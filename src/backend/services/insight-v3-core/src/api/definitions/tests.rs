use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::{Request, StatusCode};
use clickhouse::test::Mock;
use serde_json::json;
use toolkit::api::OpenApiRegistryImpl;
use tower::ServiceExt as _;

use super::*;
use crate::api::AppState;
use crate::chat::ChatClient;
use crate::definitions::Definitions;
use crate::definitions::memory::MemoryDefinitions;
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

struct TestHarness {
    /// Held, not read: the stores this harness does not exercise are built
    /// against its address, so it has to outlive them.
    _clickhouse: Mock,
    router: Router,
}

impl TestHarness {
    async fn new() -> Self {
        Self::with_caller(true).await
    }

    /// A caller who does or does not hold the admin role.
    #[allow(clippy::unused_async)]
    async fn with_caller(is_admin: bool) -> Self {
        let mut mock = Mock::new();
        mock.non_exhaustive();
        let openapi = OpenApiRegistryImpl::new();
        let url = mock.url();
        let definitions: Arc<dyn Definitions> = Arc::new(MemoryDefinitions::new());
        let state = Arc::new(AppState::new(
            RawDataStore::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(url, "insight"),
            )),
            TableStore::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(url, "insight"),
            )),
            definitions.clone(),
            MetricRunner::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(url, "insight"),
            )),
            ChatClient::canned(),
            crate::identity::IdentityClient::fixed(is_admin),
        ));
        let router = register_routes(Router::new(), &openapi, state);

        Self {
            _clickhouse: mock,
            router,
        }
    }

    async fn put_json(&self, path: &str, body: serde_json::Value) -> TestResponse {
        let request = Request::builder()
            .method("PUT")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap_or_else(
                |error| panic!("test JSON must serialize: {error}"),
            )))
            .unwrap_or_else(|error| panic!("test request must be valid: {error}"));

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .unwrap_or_else(|error| panic!("router must respond: {error}"));

        TestResponse::from_response(response).await
    }

    async fn get_json(&self, path: &str) -> TestResponse {
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .body(Body::empty())
            .unwrap_or_else(|error| panic!("test request must be valid: {error}"));

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .unwrap_or_else(|error| panic!("router must respond: {error}"));

        TestResponse::from_response(response).await
    }

    async fn delete_json(&self, path: &str) -> TestResponse {
        let request = Request::builder()
            .method("DELETE")
            .uri(path)
            .body(Body::empty())
            .unwrap_or_else(|error| panic!("test request must be valid: {error}"));

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .unwrap_or_else(|error| panic!("router must respond: {error}"));

        TestResponse::from_response(response).await
    }

    async fn list_json(&self, path: &str) -> TestResponse {
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .body(Body::empty())
            .unwrap_or_else(|error| panic!("test request must be valid: {error}"));

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .unwrap_or_else(|error| panic!("router must respond: {error}"));

        TestResponse::from_response(response).await
    }
}

struct TestResponse {
    status: StatusCode,
    body: Bytes,
}

impl TestResponse {
    async fn from_response(response: axum::response::Response) -> Self {
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap_or_else(|error| panic!("response body must be readable: {error}"));

        Self { status, body }
    }

    fn status(&self) -> StatusCode {
        self.status
    }

    #[allow(clippy::unused_async)]
    async fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|error| panic!("response body must be JSON: {error}"))
    }
}

#[tokio::test]
async fn put_then_get_returns_the_stored_body() {
    let harness = TestHarness::new().await;

    let put = harness
        .put_json("/v1/metrics/commits_per_day", json!({ "table": "events" }))
        .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    let got = harness.get_json("/v1/metrics/commits_per_day").await;
    assert_eq!(got.status(), StatusCode::OK);
    assert_eq!(got.json().await, json!({ "table": "events" }));
}

#[tokio::test]
async fn get_missing_definition_is_not_found() {
    let harness = TestHarness::new().await;

    let got = harness.get_json("/v1/metrics/nope").await;
    assert_eq!(got.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_name_outside_the_charset_is_rejected() {
    let harness = TestHarness::new().await;

    let put = harness
        .put_json("/v1/metrics/drop%20table", json!({}))
        .await;
    assert_eq!(put.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_returns_the_stored_names_in_order() {
    let harness = TestHarness::new().await;

    // Stored out of order, listed in it.
    for name in ["lines_per_day", "commits_per_day"] {
        let put = harness
            .put_json(&format!("/v1/metrics/{name}"), json!({ "table": "events" }))
            .await;
        assert_eq!(put.status(), StatusCode::NO_CONTENT);
    }

    let got = harness.list_json("/v1/metrics").await;

    assert_eq!(got.status(), StatusCode::OK);
    assert_eq!(
        got.json().await,
        json!({ "names": ["commits_per_day", "lines_per_day"] })
    );
}

#[tokio::test]
async fn put_then_get_a_widget_definition_round_trips() {
    let harness = TestHarness::new().await;

    // The widget draws this metric's columns, so it has to be there first.
    let metric = harness
        .put_json(
            "/v1/metrics/commits_per_day",
            json!({
                "table": "events",
                "fields": [{ "json": "day", "type": "string", "as_name": "day" }]
            }),
        )
        .await;
    assert_eq!(metric.status(), StatusCode::NO_CONTENT);

    let put = harness
        .put_json(
            "/v1/widgets/commits_table",
            json!({ "type": "table", "metric": "commits_per_day" }),
        )
        .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    let got = harness.get_json("/v1/widgets/commits_table").await;
    assert_eq!(got.status(), StatusCode::OK);
    assert_eq!(
        got.json().await,
        json!({ "type": "table", "metric": "commits_per_day" })
    );
}

#[tokio::test]
async fn a_caller_without_the_admin_role_reaches_nothing() {
    let harness = TestHarness::with_caller(false).await;

    // Every custom surface: the catalogues, one definition, and a write.
    for (method, path) in [
        ("GET", "/v1/metrics"),
        ("GET", "/v1/metrics/commits_per_day"),
        ("PUT", "/v1/widgets/commits_table"),
    ] {
        let response = match method {
            "PUT" => {
                harness
                    .put_json(path, json!({ "type": "table", "metric": "m" }))
                    .await
            }
            _ => harness.list_json(path).await,
        };

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} must refuse a caller without the role"
        );
    }
}

#[tokio::test]
async fn a_definition_nothing_uses_is_removed() {
    let harness = TestHarness::new().await;
    harness
        .put_json(
            "/v1/metrics/spare",
            json!({
                "table": "events",
                "fields": [{ "json": "day", "type": "string", "as_name": "day" }]
            }),
        )
        .await;

    let removed = harness.delete_json("/v1/metrics/spare").await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);

    let listed = harness.list_json("/v1/metrics").await;
    assert_eq!(listed.json().await, json!({ "names": [] }));
}

#[tokio::test]
async fn removing_what_is_not_there_says_so() {
    let harness = TestHarness::new().await;

    let removed = harness.delete_json("/v1/dashboards/never_existed").await;

    assert_eq!(removed.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_metric_a_widget_draws_is_kept_and_the_widget_named() {
    let harness = TestHarness::new().await;
    harness
        .put_json(
            "/v1/metrics/lines_per_day",
            json!({
                "table": "events",
                "fields": [{ "json": "day", "type": "string", "as_name": "day" }]
            }),
        )
        .await;
    harness
        .put_json(
            "/v1/widgets/lines_table",
            json!({ "type": "table", "metric": "lines_per_day", "columns": ["day"] }),
        )
        .await;

    let refused = harness.delete_json("/v1/metrics/lines_per_day").await;

    // A widget whose metric is gone renders an error where a chart should be.
    // A definition in use is a failed precondition, which this error family
    // answers as 400 - what matters is that the reply names the widget.
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let body = refused.json().await;
    assert!(
        body.to_string().contains("lines_table"),
        "the refusal must name what still uses it: {body}"
    );
    // And it really is still there.
    assert_eq!(
        harness.get_json("/v1/metrics/lines_per_day").await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_widget_a_dashboard_holds_is_kept_and_the_dashboard_named() {
    let harness = TestHarness::new().await;
    harness
        .put_json(
            "/v1/metrics/m",
            json!({
                "table": "events",
                "fields": [{ "json": "day", "type": "string", "as_name": "day" }]
            }),
        )
        .await;
    harness
        .put_json(
            "/v1/widgets/held",
            json!({ "type": "table", "metric": "m", "columns": ["day"] }),
        )
        .await;
    harness
        .put_json(
            "/v1/dashboards/holder",
            json!({ "title": "Holder", "widgets": ["held"] }),
        )
        .await;

    let refused = harness.delete_json("/v1/widgets/held").await;

    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert!(refused.json().await.to_string().contains("holder"));
}

#[tokio::test]
async fn a_dashboard_is_removed_even_though_widgets_point_into_it() {
    // Nothing holds a dashboard, so it is always free to remove - and its
    // widgets and metrics stay for the next one.
    let harness = TestHarness::new().await;
    harness
        .put_json(
            "/v1/dashboards/spare",
            json!({ "title": "Spare", "widgets": [] }),
        )
        .await;

    let removed = harness.delete_json("/v1/dashboards/spare").await;

    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_caller_without_the_admin_role_cannot_remove_anything() {
    let harness = TestHarness::with_caller(false).await;

    let refused = harness.delete_json("/v1/metrics/anything").await;

    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
}
