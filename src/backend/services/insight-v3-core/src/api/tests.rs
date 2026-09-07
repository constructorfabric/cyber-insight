use std::convert::Infallible;
use std::task::Poll;

use axum::body::{Body, to_bytes};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, HeaderValue, Request};
use chrono::{DateTime, Utc};
use clickhouse::test::{Mock, handlers, status};
use futures::stream;
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::json;
use toolkit::api::OpenApiRegistryImpl;
use tower::ServiceExt as _;
use uuid::Uuid;

use super::*;
use crate::raw_data::RawDataStore;

#[derive(Debug, Deserialize, clickhouse::Row)]
struct CapturedRawDataRow {
    #[serde(with = "clickhouse::serde::uuid")]
    id: Uuid,
    table_name: String,
    raw_data: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    received_at: DateTime<Utc>,
}

fn verifier() -> TokenVerifier {
    TokenVerifier::new(&SecretString::from("correct-token".to_owned()))
}

fn app(mock: &Mock) -> Router {
    let client =
        insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"));
    let state = Arc::new(AppState::new(RawDataStore::new(client)));
    let openapi = OpenApiRegistryImpl::new();

    register_routes(
        Router::new(),
        &openapi,
        state,
        IngestAdmission::new(verifier()),
    )
}

fn post(body: Body, authorization: Option<&str>) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri("/v1/raw-data")
        .header("content-type", "application/json");
    if let Some(authorization) = authorization {
        request = request.header(AUTHORIZATION, authorization);
    }

    request
        .body(body)
        .unwrap_or_else(|error| panic!("test request must be valid: {error}"))
}

fn unpollable_body() -> Body {
    Body::from_stream(stream::poll_fn(
        |_| -> Poll<Option<Result<String, Infallible>>> {
            panic!("admission middleware must not poll the request body")
        },
    ))
}

#[test]
fn bearer_scheme_is_case_insensitive() {
    for scheme in ["Bearer", "bearer", "BEARER"] {
        let mut headers = HeaderMap::new();
        let value = HeaderValue::from_str(&format!("{scheme} correct-token"))
            .unwrap_or_else(|error| panic!("test header must be valid: {error}"));
        headers.insert(AUTHORIZATION, value);

        assert!(verifier().authorizes(&headers), "scheme {scheme} must work");
    }
}

#[test]
fn malformed_or_wrong_authorization_is_rejected() {
    for value in [
        None,
        Some("Bearer"),
        Some("Bearer "),
        Some("Basic correct-token"),
        Some("Bearer wrong-token"),
        Some("Bearer correct-token extra"),
    ] {
        let mut headers = HeaderMap::new();
        if let Some(value) = value {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(value)
                    .unwrap_or_else(|error| panic!("test header must be valid: {error}")),
            );
        }

        assert!(!verifier().authorizes(&headers), "must reject: {value:?}");
    }
}

#[test]
fn duplicate_authorization_headers_are_rejected() {
    let mut headers = HeaderMap::new();
    headers.append(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer correct-token"),
    );
    headers.append(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer correct-token"),
    );

    assert!(!verifier().authorizes(&headers));
}

#[tokio::test]
async fn unauthorized_request_does_not_reach_clickhouse() {
    let mock = Mock::new();

    let response = app(&mock)
        .oneshot(post(Body::from(r#"{"table":"a","raw_data":1}"#), None))
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get(WWW_AUTHENTICATE),
        Some(&HeaderValue::from_static("Bearer"))
    );
}

#[tokio::test]
async fn authorized_request_commits_the_insert_before_returning_no_content() {
    let mock = Mock::new();
    let recording = mock.add(handlers::record::<CapturedRawDataRow>());
    let body = serde_json::to_vec(&json!({
        "table": "  synthetic.events  ",
        "raw_data": [1, {"nested": true}]
    }))
    .unwrap_or_else(|error| panic!("test JSON must serialize: {error}"));

    let response = app(&mock)
        .oneshot(post(Body::from(body), Some("Bearer correct-token")))
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));
    let rows: Vec<CapturedRawDataRow> = recording.collect().await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(rows.len(), 1);
    assert_ne!(rows[0].id, Uuid::nil());
    assert_eq!(rows[0].table_name, "synthetic.events");
    assert_eq!(rows[0].raw_data, r#"[1,{"nested":true}]"#);
    assert!(rows[0].received_at <= Utc::now());
}

#[tokio::test]
async fn invalid_table_name_is_a_client_error_without_an_insert() {
    let mock = Mock::new();

    let response = app(&mock)
        .oneshot(post(
            Body::from(r#"{"table":"   ","raw_data":null}"#),
            Some("Bearer correct-token"),
        ))
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn oversized_request_is_rejected_before_an_insert() {
    let mock = Mock::new();
    let body = format!(
        r#"{{"table":"synthetic.events","raw_data":"{}"}}"#,
        "x".repeat(MAX_REQUEST_BODY_BYTES)
    );

    let response = app(&mock)
        .oneshot(post(Body::from(body), Some("Bearer correct-token")))
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn saturated_gate_rejects_without_polling_the_body_or_clickhouse() {
    let mock = Mock::new();
    let client =
        insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"));
    let state = Arc::new(AppState::new(RawDataStore::new(client)));
    let admission = IngestAdmission::new(verifier());
    let _permits: Vec<_> = (0..MAX_CONCURRENT_WRITES)
        .map(|_| {
            admission
                .write_slots
                .clone()
                .try_acquire_owned()
                .unwrap_or_else(|error| panic!("test must saturate the gate: {error}"))
        })
        .collect();
    let openapi = OpenApiRegistryImpl::new();
    let app = register_routes(Router::new(), &openapi, state, admission);

    let unauthorized = app
        .clone()
        .oneshot(post(unpollable_body(), Some("Bearer wrong-token")))
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));
    let saturated = app
        .oneshot(post(unpollable_body(), Some("Bearer correct-token")))
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));

    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(saturated.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        saturated.headers().get(CACHE_CONTROL),
        Some(&HeaderValue::from_static("no-store"))
    );
}

#[tokio::test]
async fn missing_json_content_type_remains_unsupported_media_type() {
    let mock = Mock::new();
    let request = Request::builder()
        .method("POST")
        .uri("/v1/raw-data")
        .header(AUTHORIZATION, "Bearer correct-token")
        .body(Body::from(r#"{"table":"synthetic.events","raw_data":1}"#))
        .unwrap_or_else(|error| panic!("test request must be valid: {error}"));

    let response = app(&mock)
        .oneshot(request)
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn clickhouse_failure_returns_only_a_generic_error() {
    let mock = Mock::new();
    mock.add(handlers::failure(status::INTERNAL_SERVER_ERROR));

    let response = app(&mock)
        .oneshot(post(
            Body::from(r#"{"table":"synthetic.events","raw_data":1}"#),
            Some("Bearer correct-token"),
        ))
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));
    let status = response.status();
    let body = to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap_or_else(|error| panic!("response body must be readable: {error}"));
    let body = String::from_utf8_lossy(&body);

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body.contains("ClickHouse"));
    assert!(!body.contains("database"));
}
