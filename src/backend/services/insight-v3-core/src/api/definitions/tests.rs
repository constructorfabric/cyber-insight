use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use clickhouse::test::{Mock, handlers};
use serde_json::json;
use toolkit::api::OpenApiRegistryImpl;
use tower::ServiceExt as _;
use uuid::Uuid;

use super::*;
use crate::api::AppState;
use crate::definitions::{DefinitionRow, DefinitionStore};
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

struct TestHarness {
    mock: Mock,
    router: Router,
    table: Mutex<HashMap<String, String>>,
}

impl TestHarness {
    #[allow(clippy::unused_async)]
    async fn new() -> Self {
        let mut mock = Mock::new();
        mock.non_exhaustive();
        let openapi = OpenApiRegistryImpl::new();
        let url = mock.url();
        let state = Arc::new(AppState::new(
            RawDataStore::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(url, "insight"),
            )),
            TableStore::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(url, "insight"),
            )),
            DefinitionStore::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(url, "insight"),
            )),
            MetricRunner::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(url, "insight"),
            )),
        ));
        let router = register_routes(Router::new(), &openapi, state);

        Self {
            mock,
            router,
            table: Mutex::new(HashMap::new()),
        }
    }

    async fn put_json(&self, path: &str, body: serde_json::Value) -> TestResponse {
        let recording = self.mock.add(handlers::record::<DefinitionRow>());
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

        if response.status() == StatusCode::NO_CONTENT {
            let rows: Vec<DefinitionRow> = recording.collect().await;
            if let Some(row) = rows.into_iter().next() {
                self.table
                    .lock()
                    .unwrap_or_else(|error| panic!("test harness lock poisoned: {error}"))
                    .insert(path.to_owned(), row.body);
            }
        }

        TestResponse::from_response(response).await
    }

    async fn get_json(&self, path: &str) -> TestResponse {
        let stored = self
            .table
            .lock()
            .unwrap_or_else(|error| panic!("test harness lock poisoned: {error}"))
            .get(path)
            .cloned();

        match stored {
            Some(body) => {
                self.mock.add(handlers::provide(vec![DefinitionRow {
                    id: Uuid::now_v7(),
                    name: path.rsplit('/').next().unwrap_or_default().to_owned(),
                    body,
                    updated_at: Utc::now(),
                }]));
            }
            None => {
                self.mock
                    .add(handlers::provide(Vec::<DefinitionRow>::new()));
            }
        }

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
