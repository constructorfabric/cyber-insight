use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use clickhouse::test::{Mock, handlers};
use serde::Deserialize;
use serde_json::json;
use toolkit::api::OpenApiRegistryImpl;
use tower::ServiceExt as _;
use uuid::Uuid;

use super::*;
use crate::api::AppState;
use crate::chat::{ChatClient, Proposal};
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
    async fn new(chat: ChatClient) -> Self {
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
            chat,
        ));
        let router =
            crate::api::definitions::register_routes(Router::new(), &openapi, state.clone());
        let router = register_routes(router, &openapi, state);

        Self {
            mock,
            router,
            table: Mutex::new(HashMap::new()),
        }
    }

    fn lock_table(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.table
            .lock()
            .unwrap_or_else(|error| panic!("test harness lock poisoned: {error}"))
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
                self.lock_table().insert(path.to_owned(), row.body);
            }
        }

        TestResponse::from_response(response).await
    }

    async fn get_json(&self, path: &str) -> TestResponse {
        self.queue_stored_or_empty(path);

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

    /// Queues the ClickHouse response for a `DefinitionStore::get` lookup
    /// at `path` — the row stashed by an earlier [`Self::put_json`], or an
    /// empty result if nothing is stored there.
    fn queue_stored_or_empty(&self, path: &str) {
        let stored = self.lock_table().get(path).cloned();
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
    }

    /// Posts `message` to `/v1/chat`. The caller must have already queued a
    /// ClickHouse response for every store call `handle_chat` will make, in
    /// the order it will make them — starting with `known_tables`'s
    /// `DefinitionStore::list(Metric)`.
    async fn post_chat(&self, message: &str) -> TestResponse {
        let request = Request::builder()
            .method("POST")
            .uri("/v1/chat")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "message": message }))
                    .unwrap_or_else(|error| panic!("test JSON must serialize: {error}")),
            ))
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

#[derive(Debug, Default, Deserialize)]
#[allow(
    dead_code,
    reason = "mirrors the full response shape; not every field is asserted on"
)]
struct TestCreated {
    metric: Option<String>,
    widgets: Vec<String>,
    dashboard: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCreatedBody {
    #[serde(default)]
    #[allow(dead_code)]
    reply: String,
    #[serde(default)]
    created: TestCreated,
    #[serde(default)]
    skipped: Vec<Skipped>,
}

fn single_existing_widget_proposal() -> Proposal {
    Proposal::Create {
        reply: "ok".to_owned(),
        metric: None,
        widgets: vec![(
            "commits_table".to_owned(),
            json!({ "type": "table", "metric": "m", "columns": [] }),
        )],
        dashboard: None,
    }
}

#[tokio::test]
async fn a_name_already_in_use_is_skipped_not_overwritten() {
    let harness = TestHarness::new(ChatClient::scripted(single_existing_widget_proposal)).await;

    harness
        .put_json(
            "/v1/widgets/commits_table",
            json!({ "type": "table", "metric": "m", "columns": [] }),
        )
        .await;

    // known_tables()'s metric list, then the widget's existence check.
    harness
        .mock
        .add(handlers::provide(Vec::<DefinitionRow>::new()));
    harness.queue_stored_or_empty("/v1/widgets/commits_table");

    let response = harness.post_chat("commits_table").await;
    assert_eq!(response.status(), StatusCode::OK);
    let created: ChatCreatedBody = serde_json::from_slice(&response.body)
        .unwrap_or_else(|error| panic!("response body must be JSON: {error}"));

    assert_eq!(
        created.skipped,
        vec![Skipped {
            kind: "widget".to_owned(),
            name: "commits_table".to_owned(),
            reason: "exists".to_owned(),
        }]
    );
    assert!(created.created.widgets.is_empty());

    let stored = harness.get_json("/v1/widgets/commits_table").await;
    assert_eq!(stored.json().await["metric"], "m");
}

#[tokio::test]
async fn canned_mode_makes_no_network_call_and_stores_a_dashboard() {
    let harness = TestHarness::new(ChatClient::canned()).await;

    // known_tables()'s metric list, then a not-found lookup followed by an
    // insert for the metric, each of the two widgets, and the dashboard, in
    // that order.
    harness
        .mock
        .add(handlers::provide(Vec::<DefinitionRow>::new()));
    for _ in 0..4 {
        harness
            .mock
            .add(handlers::provide(Vec::<DefinitionRow>::new()));
        harness.mock.add(handlers::record::<DefinitionRow>());
    }

    let response = harness.post_chat("delivery report").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json().await;

    assert_eq!(body["created"]["metric"], "delivery_metric");
    assert_eq!(
        body["created"]["widgets"],
        json!(["delivery", "delivery_line"])
    );
    assert_eq!(body["created"]["dashboard"], "delivery_dashboard");
    assert_eq!(body["skipped"], json!([]));
}
