use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::{Request, StatusCode};
use clickhouse::test::{Mock, handlers};
use serde::Deserialize;
use serde_json::json;
use toolkit::api::OpenApiRegistryImpl;
use tower::ServiceExt as _;

use super::*;
use crate::api::AppState;
use crate::chat::{ChatClient, Proposal};
use crate::definitions::Definitions;
use crate::definitions::memory::MemoryDefinitions;
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

struct TestHarness {
    mock: Mock,
    router: Router,
    definitions: Arc<dyn Definitions>,
}

impl TestHarness {
    #[allow(clippy::unused_async)]
    async fn new(chat: ChatClient) -> Self {
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
            chat,
        ));
        let router =
            crate::api::definitions::register_routes(Router::new(), &openapi, state.clone());
        let router = register_routes(router, &openapi, state);

        Self {
            mock,
            router,
            definitions,
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

    /// The only ClickHouse read a chat request makes before it answers: the
    /// list of ingest tables for the prompt. The definitions it reads come
    /// from the store, which needs no priming.
    fn queue_chat_context(&self) {
        self.mock.add(handlers::provide(Vec::<String>::new()));
    }

    /// What the store holds under `name`, for the cases about what a request
    /// wrote rather than what it answered.
    async fn stored(&self, kind: DefinitionKind, name: &str) -> serde_json::Value {
        let name = crate::definitions::DefinitionName::parse(name)
            .unwrap_or_else(|error| panic!("test name must parse: {error}"));

        self.definitions
            .get(kind, &name)
            .await
            .unwrap_or_else(|error| panic!("the store must answer: {error}"))
            .unwrap_or_else(|| panic!("nothing stored under {}", name.as_str()))
    }

    /// Posts `message` to `/v1/chat`.
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
    updated: TestCreated,
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
async fn a_name_already_in_use_is_replaced_and_reported_as_updated() {
    let harness = TestHarness::new(ChatClient::scripted(single_existing_widget_proposal)).await;

    harness
        .put_json(
            "/v1/widgets/commits_table",
            json!({ "type": "table", "metric": "was_here_first", "columns": [] }),
        )
        .await;

    harness.queue_chat_context();

    let response = harness.post_chat("commits_table").await;
    assert_eq!(response.status(), StatusCode::OK);
    let created: ChatCreatedBody = serde_json::from_slice(&response.body)
        .unwrap_or_else(|error| panic!("response body must be JSON: {error}"));

    // Reusing a name is how the reader changes something by asking, so the
    // write goes through and the reply says which names it replaced.
    assert_eq!(created.updated.widgets, vec!["commits_table".to_owned()]);
    assert!(created.created.widgets.is_empty());

    let stored = harness
        .stored(DefinitionKind::Widget, "commits_table")
        .await;
    assert_eq!(stored["metric"], "m", "the new body must have replaced it");
}

#[tokio::test]
async fn canned_mode_makes_no_network_call_and_stores_a_dashboard() {
    let harness = TestHarness::new(ChatClient::canned()).await;

    harness.queue_chat_context();

    let response = harness.post_chat("delivery report").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json().await;

    assert_eq!(body["created"]["metric"], "delivery_metric");
    assert_eq!(
        body["created"]["widgets"],
        json!(["delivery", "delivery_line"])
    );
    assert_eq!(body["created"]["dashboard"], "delivery_dashboard");
    assert_eq!(
        body["updated"],
        json!({ "metric": null, "widgets": [], "dashboard": null })
    );
}
