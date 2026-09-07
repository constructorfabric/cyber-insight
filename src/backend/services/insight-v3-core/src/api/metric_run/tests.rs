use std::sync::Arc;

use axum::body::{Body, Bytes, to_bytes};
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use clickhouse::test::{Mock, handlers};
use serde_json::json;
use tower::ServiceExt as _;
use uuid::Uuid;

use super::*;
use crate::api::AppState;
use crate::definitions::{DefinitionRow, DefinitionStore};
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

type R = Result<(), Box<dyn std::error::Error>>;

struct TestHarness {
    mock: Mock,
    router: Router,
}

impl TestHarness {
    #[allow(clippy::unused_async)]
    async fn new(metrics_url: &str) -> Self {
        let mut mock = Mock::new();
        mock.non_exhaustive();
        let openapi = toolkit::api::OpenApiRegistryImpl::new();
        let definitions_url = mock.url();
        let metrics_client = insight_clickhouse::Client::new(insight_clickhouse::Config::new(
            metrics_url,
            "insight",
        ));
        let state = Arc::new(AppState::new(
            RawDataStore::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(definitions_url, "insight"),
            )),
            TableStore::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(definitions_url, "insight"),
            )),
            DefinitionStore::new(insight_clickhouse::Client::new(
                insight_clickhouse::Config::new(definitions_url, "insight"),
            )),
            MetricRunner::new(metrics_client),
        ));
        let router = register_routes(Router::new(), &openapi, state);

        Self { mock, router }
    }

    async fn run(&self, name: &str, stored: Option<serde_json::Value>) -> TestResponse {
        match stored {
            Some(body) => {
                self.mock.add(handlers::provide(vec![DefinitionRow {
                    id: Uuid::now_v7(),
                    name: name.to_owned(),
                    body: serde_json::to_string(&body)
                        .unwrap_or_else(|error| panic!("test JSON must serialize: {error}")),
                    updated_at: Utc::now(),
                }]));
            }
            None => {
                self.mock
                    .add(handlers::provide(Vec::<DefinitionRow>::new()));
            }
        }

        let request = Request::builder()
            .method("POST")
            .uri(format!("/v1/metrics/{name}/run"))
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
async fn a_stored_metric_runs_and_returns_columns_in_field_order() -> R {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let upstream = Router::new().route(
        "/",
        post(|| async {
            Json(json!({
                "meta": [{"name": "day", "type": "String"}, {"name": "lines", "type": "UInt64"}],
                "data": [{"day": "2026-09-01", "lines": 3}]
            }))
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await });

    let harness = TestHarness::new(&format!("http://{address}")).await;
    let metric = json!({
        "table": "events",
        "fields": [
            { "json": "day", "type": "string", "as_name": "day" },
            { "json": "lines", "type": "int", "agg": "sum", "as_name": "lines" }
        ],
        "group_by": ["day"],
        "filters": [],
        "limit": 100
    });

    let response = harness.run("commits_per_day", Some(metric)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json().await,
        json!({ "columns": ["day", "lines"], "rows": [["2026-09-01", 3]] })
    );

    server.abort();
    Ok(())
}

#[tokio::test]
async fn a_missing_metric_is_not_found() -> R {
    let harness = TestHarness::new("http://127.0.0.1:1").await;

    let response = harness.run("nope", None).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn an_uncompilable_metric_is_a_bad_request() -> R {
    let harness = TestHarness::new("http://127.0.0.1:1").await;
    let metric = json!({ "table": "events", "fields": [], "group_by": [], "filters": [] });

    let response = harness.run("empty", Some(metric)).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn a_name_outside_the_charset_is_rejected() -> R {
    let harness = TestHarness::new("http://127.0.0.1:1").await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/metrics/drop%20table/run")
        .body(Body::empty())?;
    let response = harness.router.clone().oneshot(request).await?;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    Ok(())
}
