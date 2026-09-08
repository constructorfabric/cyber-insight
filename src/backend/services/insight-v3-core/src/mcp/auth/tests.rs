use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::http::StatusCode;
use axum::http::header::WWW_AUTHENTICATE;
use axum::routing::get;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use jsonwebtoken::{EncodingKey, Header, encode};
use p256::SecretKey;
use p256::elliptic_curve::Generate as _;
use p256::elliptic_curve::sec1::ToSec1Point as _;
use p256::pkcs8::{EncodePrivateKey as _, LineEnding};
use serde_json::{Value, json};
use tokio::task::JoinHandle;

use super::*;

struct Issuer {
    origin: String,
    key: EncodingKey,
    server: JoinHandle<()>,
}

impl Issuer {
    async fn start() -> Self {
        let secret = SecretKey::generate();

        let Ok(pem) = secret.to_pkcs8_pem(LineEnding::LF) else {
            panic!("a generated key encodes as PKCS#8 PEM");
        };
        let Ok(key) = EncodingKey::from_ec_pem(pem.as_bytes()) else {
            panic!("a PKCS#8 PEM is a usable EC signing key");
        };

        let point = secret.public_key().to_sec1_point(false);
        let (Some(x), Some(y)) = (point.x(), point.y()) else {
            panic!("an uncompressed SEC1 point carries both coordinates");
        };

        let jwks = json!({
            "keys": [{
                "kty": "EC",
                "crv": "P-256",
                "use": "sig",
                "alg": "ES256",
                "kid": "test-key",
                "x": B64.encode(x),
                "y": B64.encode(y),
            }]
        });

        let app = axum::Router::new().route(
            "/.well-known/jwks.json",
            get(move || {
                let jwks = jwks.clone();
                async move { Json(jwks) }
            }),
        );

        let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
            panic!("an ephemeral loopback port is bindable");
        };
        let Ok(address) = listener.local_addr() else {
            panic!("a bound listener has a local address");
        };
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self {
            origin: format!("http://{address}"),
            key,
            server,
        }
    }

    fn verifier(&self) -> TokenVerifier {
        let Ok(verifier) = TokenVerifier::new(&self.origin, true) else {
            panic!("a loopback origin is an allowed public URL");
        };

        verifier
    }

    fn claims(&self) -> Value {
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            panic!("the clock is after the epoch");
        };
        let now = now.as_secs();

        json!({
            "sub": "test-user",
            "tenant_id": "test-tenant",
            "roles": "user admin",
            "sub_type": "user",
            "sid": "test-session",
            "iss": self.origin,
            "aud": format!("{}{MCP_PATH}", self.origin),
            "scope": format!("openid {MCP_SCOPE}"),
            "iat": now,
            "exp": now + 600,
            "jti": "test-token",
        })
    }

    fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some("test-key".to_owned());

        let Ok(token) = encode(&header, claims, &self.key) else {
            panic!("the claims encode against the signing key");
        };

        token
    }

    fn stop(self) {
        self.server.abort();
    }
}

#[tokio::test]
async fn a_token_issued_for_this_server_by_an_administrator_is_accepted() {
    let issuer = Issuer::start().await;

    let token = issuer.sign(&issuer.claims());

    assert!(issuer.verifier().verify(&token).await.is_ok());

    issuer.stop();
}

#[tokio::test]
async fn a_token_minted_for_another_resource_is_refused() {
    let issuer = Issuer::start().await;

    let mut claims = issuer.claims();
    claims["aud"] = json!(format!("{}/mcp", issuer.origin));
    let token = issuer.sign(&claims);

    let Err(failure) = issuer.verifier().verify(&token).await else {
        panic!("a token for the read-only server does not open this one");
    };
    assert!(matches!(failure, AuthFailure::Unauthorized), "{failure:?}");

    issuer.stop();
}

#[tokio::test]
async fn a_token_carrying_only_the_read_only_scope_is_refused() {
    let issuer = Issuer::start().await;

    let mut claims = issuer.claims();
    claims["scope"] = json!("openid mcp:query");
    let token = issuer.sign(&claims);

    let Err(failure) = issuer.verifier().verify(&token).await else {
        panic!("the read-only scope does not authorize authoring");
    };
    assert!(
        matches!(failure, AuthFailure::InsufficientScope),
        "{failure:?}"
    );

    issuer.stop();
}

#[tokio::test]
async fn a_token_whose_roles_do_not_include_admin_is_refused() {
    let issuer = Issuer::start().await;

    let mut claims = issuer.claims();
    claims["roles"] = json!("user");
    let token = issuer.sign(&claims);

    let Err(failure) = issuer.verifier().verify(&token).await else {
        panic!("the custom surfaces are administrator-only");
    };
    assert!(
        matches!(failure, AuthFailure::InsufficientScope),
        "{failure:?}"
    );

    issuer.stop();
}

#[tokio::test]
async fn a_token_for_a_service_rather_than_a_person_is_refused() {
    let issuer = Issuer::start().await;

    let mut claims = issuer.claims();
    claims["sub_type"] = json!("service");
    let token = issuer.sign(&claims);

    let Err(failure) = issuer.verifier().verify(&token).await else {
        panic!("an MCP grant belongs to a person, not a service");
    };
    assert!(
        matches!(failure, AuthFailure::InsufficientScope),
        "{failure:?}"
    );

    issuer.stop();
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let issuer = Issuer::start().await;

    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        panic!("the clock is after the epoch");
    };
    let now = now.as_secs();

    let mut claims = issuer.claims();
    claims["iat"] = json!(now - 1200);
    claims["exp"] = json!(now - 600);
    let token = issuer.sign(&claims);

    let Err(failure) = issuer.verifier().verify(&token).await else {
        panic!("an expired token is not a credential");
    };
    assert!(matches!(failure, AuthFailure::Unauthorized), "{failure:?}");

    issuer.stop();
}

#[tokio::test]
async fn a_token_from_another_issuer_is_refused() {
    let issuer = Issuer::start().await;

    let mut claims = issuer.claims();
    claims["iss"] = json!("https://elsewhere.example.invalid");
    let token = issuer.sign(&claims);

    let Err(failure) = issuer.verifier().verify(&token).await else {
        panic!("only this gateway issues tokens this server accepts");
    };
    assert!(matches!(failure, AuthFailure::Unauthorized), "{failure:?}");

    issuer.stop();
}

#[tokio::test]
async fn a_token_that_is_not_a_token_at_all_is_refused() {
    let issuer = Issuer::start().await;

    let Err(failure) = issuer.verifier().verify("not-a-token").await else {
        panic!("an unparseable bearer is not a credential");
    };
    assert!(matches!(failure, AuthFailure::Unauthorized), "{failure:?}");

    issuer.stop();
}

#[test]
fn a_public_url_that_is_not_an_origin_this_server_can_trust_is_refused() {
    for raw in [
        "",
        "not-a-url",
        "ftp://insight.example.invalid",
        "https://insight.example.invalid/path",
        "https://insight.example.invalid/?query=1",
        "https://user:secret@insight.example.invalid",
        "http://insight.example.invalid",
    ] {
        assert!(
            validate_public_url(raw, false).is_err(),
            "should reject: {raw:?}"
        );
    }
}

#[test]
fn an_https_origin_and_a_loopback_origin_are_both_allowed() {
    assert!(validate_public_url("https://insight.example.invalid", false).is_ok());
    assert!(validate_public_url("http://localhost:3000", false).is_ok());
}

#[test]
fn a_challenge_names_this_server_s_own_metadata_document_and_scope() {
    let Ok(verifier) = TokenVerifier::new("https://insight.example.invalid", false) else {
        panic!("a plain https origin is a valid public URL");
    };

    let response = verifier.challenge(StatusCode::UNAUTHORIZED, "invalid_token");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let Some(header) = response
        .headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
    else {
        panic!("a challenge carries WWW-Authenticate");
    };
    assert!(
        header.contains("oauth-protected-resource/mcp/v3"),
        "{header}"
    );
    assert!(header.contains(MCP_SCOPE), "{header}");
    assert!(header.contains("invalid_token"), "{header}");
}
