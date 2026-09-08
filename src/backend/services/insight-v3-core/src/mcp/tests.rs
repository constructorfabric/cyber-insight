use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, Request, StatusCode};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt as _;

use super::*;
use crate::api::AppState;
use crate::catalog::Catalog;
use crate::chat::ChatClient;
use crate::config::McpConfig;
use crate::definitions::memory::MemoryDefinitions;
use crate::identity::IdentityClient;
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

type R = Result<(), Box<dyn Error>>;

fn surfaces() -> tools::CustomSurfaces {
    let client = || {
        insight_clickhouse::Client::new(insight_clickhouse::Config::new(
            "http://clickhouse.invalid",
            "insight",
        ))
    };

    let Ok(identity) = IdentityClient::new("http://identity.invalid") else {
        panic!("a plain http base URL builds an identity client");
    };

    let state = Arc::new(AppState::new(
        RawDataStore::new(client()),
        TableStore::new(client()),
        Arc::new(MemoryDefinitions::new()),
        MetricRunner::new(client()),
        ChatClient::canned(),
        identity,
        Catalog::new(client(), "insight".to_owned()),
    ));

    tools::CustomSurfaces::new(state)
}

fn enabled() -> McpConfig {
    McpConfig {
        enabled: true,
        bind_addr: "127.0.0.1:0".to_owned(),
        public_url: "http://localhost:3000".to_owned(),
        allow_insecure_private_network: false,
    }
}

fn call(authorization: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(auth::MCP_PATH)
        .header("host", "localhost")
        .header("content-type", "application/json");

    if let Some(value) = authorization {
        builder = builder.header(AUTHORIZATION, HeaderValue::from_str(value)?);
    }

    Ok(builder.body(Body::from(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    ))?)
}

#[tokio::test]
async fn a_disabled_server_binds_nothing_and_reports_success() -> R {
    start(&McpConfig::default(), surfaces(), CancellationToken::new()).await?;

    Ok(())
}

#[tokio::test]
async fn an_enabled_server_with_no_public_url_refuses_to_start() {
    let config = McpConfig {
        enabled: true,
        ..McpConfig::default()
    };

    assert!(
        start(&config, surfaces(), CancellationToken::new())
            .await
            .is_err(),
        "a server with no origin cannot verify a token and must not listen"
    );
}

#[tokio::test]
async fn an_enabled_server_binds_its_address_and_stops_when_cancelled() -> R {
    let cancellation = CancellationToken::new();

    start(&enabled(), surfaces(), cancellation.clone()).await?;
    cancellation.cancel();

    Ok(())
}

#[tokio::test]
async fn a_request_carrying_no_bearer_token_is_challenged() -> R {
    let router = router(&enabled(), surfaces(), CancellationToken::new())?;

    let response = router.oneshot(call(None)?).await?;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let Some(challenge) = response
        .headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
    else {
        panic!("a challenge tells the client where to authorize");
    };
    assert!(
        challenge.contains("oauth-protected-resource/mcp/v3"),
        "{challenge}"
    );
    assert!(challenge.contains(auth::MCP_SCOPE), "{challenge}");

    Ok(())
}

#[tokio::test]
async fn a_request_whose_authorization_is_not_a_bearer_is_challenged() -> R {
    let router = router(&enabled(), surfaces(), CancellationToken::new())?;

    let response = router.oneshot(call(Some("Basic abc"))?).await?;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}

#[tokio::test]
async fn a_bearer_this_server_cannot_verify_does_not_reach_the_tools() -> R {
    let router = router(&enabled(), surfaces(), CancellationToken::new())?;

    let response = router.oneshot(call(Some("Bearer not-a-token"))?).await?;

    assert!(
        matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::SERVICE_UNAVAILABLE
        ),
        "unexpected status: {}",
        response.status()
    );

    Ok(())
}

#[test]
fn a_bearer_header_yields_its_token_and_anything_else_yields_none() {
    let mut headers = axum::http::HeaderMap::new();
    assert_eq!(auth::bearer_token(&headers), None);

    for value in ["Basic abc", "Bearer ", "Bearer two words", "bearer-abc"] {
        let Ok(header) = HeaderValue::from_str(value) else {
            panic!("the fixture is a valid header value: {value}");
        };
        headers.insert(AUTHORIZATION, header);
        assert_eq!(
            auth::bearer_token(&headers),
            None,
            "should reject: {value:?}"
        );
    }

    headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer abc"));
    assert_eq!(auth::bearer_token(&headers), Some("abc"));
}
