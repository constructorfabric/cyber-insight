//! The chat endpoint: answers a question from the data, or writes
//! metric/widget/dashboard definitions.

use std::sync::Arc;

use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use toolkit::api::{OpenApiRegistry, OperationBuilder};
use toolkit_canonical_errors::{CanonicalError, resource_error};
use utoipa::ToSchema;

use super::AppState;
use crate::chat::{Catalogue, ChatError, KnownTable, Proposal, Turn};
use crate::definitions::{DefinitionError, DefinitionKind, DefinitionName, DefinitionStoreError};
use crate::metric_query::{MetricQueryError, RunResult};
use crate::tables::TableName;

#[resource_error("gts.cf.insight.insight_v3_core.chat.v1~")]
struct ChatApiError;

#[derive(Debug, Deserialize, ToSchema)]
struct ChatRequest {
    message: String,
    /// The turns before this one. The service keeps no session, so the panel
    /// sends the thread back; without it the model answered a follow-up with
    /// no idea what came before it.
    #[serde(default)]
    history: Vec<Turn>,
}
impl toolkit::api::api_dto::RequestApiDto for ChatRequest {}

#[derive(Debug, Serialize)]
struct ChatAnswerResponse {
    reply: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<RunResult>,
}

#[derive(Debug, Serialize)]
struct ChatCreatedResponse {
    reply: String,
    created: CreatedNames,
    /// What already existed under these names and now holds something else.
    updated: CreatedNames,
}

#[derive(Debug, Default, Serialize)]
struct CreatedNames {
    metric: Option<String>,
    widgets: Vec<String>,
    dashboard: Option<String>,
}

pub(crate) fn register_routes(
    router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
) -> Router {
    let chat = OperationBuilder::post("/v1/chat")
        .operation_id("insight_v3_core.chat")
        .summary("Answer a question from the data, or write metric/widget/dashboard definitions")
        .anonymous()
        .exposed()
        .json_request::<ChatRequest>(openapi, "The chat message")
        .json_response(StatusCode::OK, "The model's reply")
        .error_400(openapi)
        .error_500(openapi)
        .error_504(openapi)
        .handler(handle_chat)
        .register(Router::new(), openapi)
        .layer(Extension(state));

    router.merge(chat)
}

async fn handle_chat(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<ChatRequest>,
) -> Result<Response, CanonicalError> {
    let tables = known_tables(&state).await;
    let catalogue = catalogue(&state).await;
    let proposal = state
        .chat()
        .propose(&request.message, &request.history, &tables, &catalogue)
        .await
        .map_err(chat_error)?;

    match proposal {
        Proposal::Answer { reply, query } => {
            // No query means the reply stands on its own - a question about
            // what data exists is answered by the table list in the prompt.
            let result = match query {
                Some(query) => {
                    let compiled = query.compile().map_err(|error| compile_error(&error))?;
                    Some(state.metrics().run(&compiled).await.map_err(run_error)?)
                }
                None => None,
            };

            Ok(Json(ChatAnswerResponse { reply, result }).into_response())
        }
        Proposal::Create {
            reply,
            metric,
            widgets,
            dashboard,
        } => {
            let mut asked = Vec::new();
            if let Some((name, body)) = metric {
                asked.push((DefinitionKind::Metric, name, body));
            }
            for (name, body) in widgets {
                asked.push((DefinitionKind::Widget, name, body));
            }
            if let Some((name, body)) = dashboard {
                asked.push((DefinitionKind::Dashboard, name, body));
            }

            // Every name is checked before anything is written, so one bad
            // name in the set stores none of it.
            let mut writes = Vec::with_capacity(asked.len());
            for (kind, name, body) in asked {
                let parsed = DefinitionName::parse(&name).map_err(definition_error)?;
                writes.push((kind, parsed, body, name));
            }

            // A widget draws its metric's columns by the names the metric
            // gives them. Unchecked, a widget could name a column that is not
            // there and the chart drew its axes and no line.
            for (kind, _, body, _) in &writes {
                if *kind == DefinitionKind::Widget {
                    check_widget_in_batch(&state, body, &writes).await?;
                }
            }

            // Which names are new is read before the write, so a name another
            // writer takes in between is reported as created rather than
            // replaced. The write itself is one transaction either way.
            let mut created = CreatedNames::default();
            let mut updated = CreatedNames::default();
            for (kind, parsed, _, name) in &writes {
                let held = state
                    .definitions()
                    .get(*kind, parsed)
                    .await
                    .map_err(definition_store_error)?
                    .is_some();
                let names = if held { &mut updated } else { &mut created };
                match kind {
                    DefinitionKind::Metric => names.metric = Some(name.clone()),
                    DefinitionKind::Widget => names.widgets.push(name.clone()),
                    DefinitionKind::Dashboard => names.dashboard = Some(name.clone()),
                }
            }

            let batch: Vec<_> = writes
                .into_iter()
                .map(|(kind, parsed, body, _)| (kind, parsed, body))
                .collect();
            state
                .definitions()
                .put_all(&batch)
                .await
                .map_err(definition_store_error)?;

            Ok(Json(ChatCreatedResponse {
                reply,
                created,
                updated,
            })
            .into_response())
        }
    }
}

/// What is already stored, so the model can name it, reuse it, and replace it
/// when the reader asks for a change. A listing failure degrades the hint; it
/// does not fail the chat.
async fn catalogue(state: &AppState) -> Catalogue {
    Catalogue {
        metrics: names(state, DefinitionKind::Metric).await,
        widgets: names(state, DefinitionKind::Widget).await,
        dashboards: names(state, DefinitionKind::Dashboard).await,
    }
}

async fn names(state: &AppState, kind: DefinitionKind) -> Vec<String> {
    match state.definitions().list(kind).await {
        Ok(names) => names,
        Err(error) => {
            tracing::warn!(error = ?error, ?kind, "could not list definitions for the chat");
            Vec::new()
        }
    }
}

/// Checks a widget against the metric it draws, whether that metric is
/// already stored or arriving in the same request.
///
/// A chat request usually builds the metric and the widget together, so the
/// metric is not in the store yet when the widget is checked.
async fn check_widget_in_batch(
    state: &AppState,
    body: &serde_json::Value,
    batch: &[(DefinitionKind, DefinitionName, serde_json::Value, String)],
) -> Result<(), CanonicalError> {
    let widget: crate::widget::Widget = serde_json::from_value(body.clone())
        .map_err(|error| crate::api::definitions::widget_error(&error.into()))?;

    let arriving = batch
        .iter()
        .find(|(kind, _, _, name)| *kind == DefinitionKind::Metric && name == widget.metric());

    match arriving {
        Some((_, _, metric_body, _)) => {
            let metric: crate::metric_query::MetricQuery =
                serde_json::from_value(metric_body.clone())
                    .map_err(|error| crate::api::definitions::widget_error(&error.into()))?;
            widget
                .check_against(&metric)
                .map_err(|error| crate::api::definitions::widget_error(&error))
        }
        None => crate::api::definitions::check_widget(state, body).await,
    }
}

/// The tables the reader has data in, each with the field names and types
/// `TableStore::sample_fields` found in its most recent rows. A table with
/// nothing in it is left out.
///
/// Read from the ingested tables themselves. Deriving them from the stored
/// metrics instead meant a stand with no metrics yet told the model there
/// was no data at all — so asked what data existed, it invented
/// `information_schema` and the query failed in the database. A listing
/// failure degrades the hint; it does not fail the chat.
async fn known_tables(state: &AppState) -> Vec<KnownTable> {
    let names = match state.tables().list().await {
        Ok(names) => names,
        Err(error) => {
            tracing::warn!(error = ?error, "could not list tables to seed chat table hints");
            return Vec::new();
        }
    };

    let mut described = Vec::with_capacity(names.len());
    for name in names {
        let Ok(table_name) = TableName::parse(&name) else {
            continue;
        };
        let fields = state
            .tables()
            .sample_fields(&table_name)
            .await
            .unwrap_or_default();

        // Nothing has landed here, so there are no fields to query and
        // naming it only crowds the list the reader is shown.
        if fields.is_empty() {
            continue;
        }

        described.push(KnownTable {
            fields: fields
                .iter()
                .map(|(field, kind)| format!("{field} ({kind})"))
                .collect::<Vec<_>>()
                .join(", "),
            name,
        });
    }

    described
}

fn chat_error(error: ChatError) -> CanonicalError {
    match error {
        ChatError::Json(source) => ChatApiError::invalid_argument()
            .with_field_violation("reply", source.to_string(), "INVALID")
            .create(),
        ChatError::Metric(source) => ChatApiError::invalid_argument()
            .with_field_violation("reply", source.to_string(), "INVALID")
            .create(),
        ChatError::UnknownTable { .. } => ChatApiError::invalid_argument()
            .with_field_violation("query", error.to_string(), "INVALID")
            .create(),
        ChatError::EmptyCreate => ChatApiError::invalid_argument()
            .with_field_violation("reply", ChatError::EmptyCreate.to_string(), "INVALID")
            .create(),
        ChatError::TokenRejected => {
            tracing::error!("the configured anthropic token was rejected upstream");
            CanonicalError::internal("chat is not configured correctly").create()
        }
        ChatError::Unavailable => {
            tracing::warn!("the model was unavailable");
            CanonicalError::internal("the model is unavailable right now").create()
        }
        ChatError::Timeout => {
            ChatApiError::deadline_exceeded("the model did not answer in time").create()
        }
        ChatError::Failed => {
            tracing::error!("the model call failed");
            CanonicalError::internal("chat failed").create()
        }
    }
}

fn compile_error(error: &MetricQueryError) -> CanonicalError {
    ChatApiError::invalid_argument()
        .with_field_violation("query", error.to_string(), "INVALID")
        .create()
}

fn run_error(error: crate::metric_query::MetricRunError) -> CanonicalError {
    use crate::metric_query::MetricRunError;

    match error {
        MetricRunError::Timeout => ChatApiError::deadline_exceeded("query timed out").create(),
        MetricRunError::ResultTooLarge => ChatApiError::invalid_argument()
            .with_field_violation("query", "result exceeded the size limit", "TOO_LARGE")
            .create(),
        MetricRunError::ClickHouse(source) => {
            tracing::error!(error = ?source, "chat query execution failed");
            CanonicalError::internal("chat query execution failed").create()
        }
        MetricRunError::InvalidResponse(source) => {
            tracing::error!(error = ?source, "chat query result deserialization failed");
            CanonicalError::internal("chat query execution failed").create()
        }
    }
}

fn definition_error(error: DefinitionError) -> CanonicalError {
    ChatApiError::invalid_argument()
        .with_field_violation("name", error.to_string(), "INVALID")
        .create()
}

fn definition_store_error(error: DefinitionStoreError) -> CanonicalError {
    match error {
        // Waiting for a connection is the store being busy, not broken.
        DefinitionStoreError::Database(sea_orm::DbErr::ConnectionAcquire(source)) => {
            tracing::warn!(error = ?source, "definition store connection timed out");
            ChatApiError::deadline_exceeded("definition store timed out").create()
        }
        DefinitionStoreError::Database(source) => {
            tracing::error!(error = ?source, "definition store operation failed");
            CanonicalError::internal("definition store operation failed").create()
        }
        DefinitionStoreError::Json(source) => {
            tracing::error!(error = ?source, "definition body serialization failed");
            CanonicalError::internal("definition store operation failed").create()
        }
    }
}

#[cfg(test)]
mod tests;
