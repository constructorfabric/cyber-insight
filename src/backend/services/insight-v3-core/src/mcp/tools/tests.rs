use std::error::Error;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use serde_json::{Value, json};

use super::*;
use crate::api::AppState;
use crate::catalog::Catalog;
use crate::chat::ChatClient;
use crate::definitions::memory::MemoryDefinitions;
use crate::identity::IdentityClient;
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

type R = Result<(), Box<dyn Error>>;

fn surfaces() -> CustomSurfaces {
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

    CustomSurfaces::new(state)
}

fn metric_body() -> Value {
    json!({
        "table": "events",
        "fields": [
            {"json": "actor", "type": "string", "as_name": "actor"},
            {"json": "actor", "type": "string", "agg": "count", "as_name": "total"}
        ],
        "group_by": ["actor"]
    })
}

fn put(name: &str, body: Value) -> Parameters<PutRequest> {
    Parameters(PutRequest {
        name: name.to_owned(),
        body,
    })
}

fn named(kind: ToolKind, name: &str) -> Parameters<NamedRequest> {
    Parameters(NamedRequest {
        kind,
        name: name.to_owned(),
    })
}

fn error_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn assert_refused(result: &CallToolResult, expected: &str) {
    assert_eq!(result.is_error, Some(true), "should be refused: {result:?}");
    let text = error_text(result);
    assert!(
        text.contains(expected),
        "should mention {expected:?}: {text}"
    );
}

fn assert_accepted(result: &CallToolResult) -> Value {
    assert_ne!(
        result.is_error,
        Some(true),
        "should be accepted: {result:?}"
    );
    let Some(value) = result.structured_content.clone() else {
        panic!("an accepted call carries structured content: {result:?}");
    };

    value
}

#[test]
fn the_server_announces_exactly_the_eight_custom_surface_tools() {
    let tools = CustomSurfaces::tool_router().list_all();

    let mut names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();

    assert_eq!(
        names,
        [
            "delete_definition",
            "get_definition",
            "list_definitions",
            "list_tables",
            "put_dashboard",
            "put_metric",
            "put_widget",
            "run_metric",
        ]
    );
}

#[test]
fn every_tool_describes_itself_so_a_client_knows_when_to_reach_for_it() {
    for tool in CustomSurfaces::tool_router().list_all() {
        let Some(description) = tool.description.as_ref() else {
            panic!("tool {} has no description", tool.name);
        };
        assert!(
            description.len() > 30,
            "tool {} is described too thinly: {description}",
            tool.name
        );
    }
}

#[test]
fn the_instructions_point_a_client_at_the_discovery_tool_first() {
    let info = rmcp::ServerHandler::get_info(&surfaces());

    let Some(instructions) = info.instructions else {
        panic!("the server carries instructions");
    };
    assert!(instructions.contains("list_tables"), "{instructions}");
}

#[tokio::test]
async fn a_stored_metric_is_listed_and_read_back() -> R {
    let surfaces = surfaces();

    assert_accepted(&surfaces.put_metric(put("per-actor", metric_body())).await);

    let listed = assert_accepted(
        &surfaces
            .list_definitions(Parameters(KindRequest {
                kind: ToolKind::Metric,
            }))
            .await,
    );
    assert_eq!(listed, json!({"names": ["per-actor"]}));

    let read = assert_accepted(
        &surfaces
            .get_definition(named(ToolKind::Metric, "per-actor"))
            .await,
    );
    assert_eq!(read, metric_body());

    Ok(())
}

#[tokio::test]
async fn reading_a_definition_that_was_never_stored_says_so() {
    let result = surfaces()
        .get_definition(named(ToolKind::Dashboard, "absent"))
        .await;

    assert_refused(&result, "was not found");
}

#[tokio::test]
async fn a_name_the_store_would_not_accept_is_refused_before_any_read() {
    let result = surfaces()
        .get_definition(named(ToolKind::Metric, "not a valid name"))
        .await;

    assert_refused(&result, "definition names");
}

#[tokio::test]
async fn a_widget_drawing_a_column_its_metric_does_not_produce_is_refused() -> R {
    let surfaces = surfaces();
    surfaces.put_metric(put("per-actor", metric_body())).await;

    let result = surfaces
        .put_widget(put(
            "chart",
            json!({"type": "line", "metric": "per-actor", "x": "actor", "y": "lines"}),
        ))
        .await;

    assert_refused(&result, "lines");

    Ok(())
}

#[tokio::test]
async fn a_widget_naming_a_metric_that_is_not_stored_is_refused() {
    let result = surfaces()
        .put_widget(put(
            "chart",
            json!({"type": "table", "metric": "absent", "columns": []}),
        ))
        .await;

    assert_refused(&result, "no metric named");
}

#[tokio::test]
async fn a_metric_a_widget_still_draws_is_not_deleted() -> R {
    let surfaces = surfaces();
    surfaces.put_metric(put("per-actor", metric_body())).await;
    surfaces
        .put_widget(put(
            "chart",
            json!({"type": "line", "metric": "per-actor", "x": "actor", "y": "total"}),
        ))
        .await;

    let result = surfaces
        .delete_definition(named(ToolKind::Metric, "per-actor"))
        .await;

    assert_refused(&result, "chart");

    Ok(())
}

#[tokio::test]
async fn a_dashboard_is_stored_and_then_removed() -> R {
    let surfaces = surfaces();
    assert_accepted(
        &surfaces
            .put_dashboard(put(
                "board",
                json!({"title": "Example board", "widgets": []}),
            ))
            .await,
    );

    assert_accepted(
        &surfaces
            .delete_definition(named(ToolKind::Dashboard, "board"))
            .await,
    );

    let listed = assert_accepted(
        &surfaces
            .list_definitions(Parameters(KindRequest {
                kind: ToolKind::Dashboard,
            }))
            .await,
    );
    assert_eq!(listed, json!({"names": []}));

    Ok(())
}

#[tokio::test]
async fn deleting_a_definition_that_was_never_stored_says_so() {
    let result = surfaces()
        .delete_definition(named(ToolKind::Widget, "absent"))
        .await;

    assert_refused(&result, "was not found");
}

#[tokio::test]
async fn running_a_metric_that_was_never_stored_says_so() {
    let result = surfaces()
        .run_metric(Parameters(NameRequest {
            name: "absent".to_owned(),
        }))
        .await;

    assert_refused(&result, "was not found");
}

#[tokio::test]
async fn a_catalogue_that_cannot_be_read_is_a_tool_error_rather_than_a_panic() {
    let result = surfaces().list_tables().await;

    assert_eq!(result.is_error, Some(true), "{result:?}");
}
