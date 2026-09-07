//! The chat endpoint: answers a question from the data, or writes
//! metric/widget/dashboard definitions.

use std::sync::Arc;

use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use toolkit::api::{OpenApiRegistry, OperationBuilder};
use toolkit_canonical_errors::{CanonicalError, resource_error};
use utoipa::ToSchema;

use super::AppState;
use crate::chat::{ChatError, KnownTable, Proposal};
use crate::definitions::{DefinitionError, DefinitionKind, DefinitionName, DefinitionStoreError};
use crate::metric_query::{MetricQueryError, RunResult};
use crate::tables::TableName;

#[resource_error("gts.cf.insight.insight_v3_core.chat.v1~")]
struct ChatApiError;

#[derive(Debug, Deserialize, ToSchema)]
struct ChatRequest {
    message: String,
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
    skipped: Vec<Skipped>,
}

#[derive(Debug, Default, Serialize)]
struct CreatedNames {
    metric: Option<String>,
    widgets: Vec<String>,
    dashboard: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Skipped {
    kind: String,
    name: String,
    reason: String,
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
    let proposal = state
        .chat()
        .propose(&request.message, &tables)
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
            let mut created = CreatedNames::default();
            let mut skipped = Vec::new();

            if let Some((name, body)) = metric {
                match store_definition(&state, DefinitionKind::Metric, "metric", name, body).await?
                {
                    StoreOutcome::Created(name) => created.metric = Some(name),
                    StoreOutcome::Skipped(entry) => skipped.push(entry),
                }
            }

            for (name, body) in widgets {
                match store_definition(&state, DefinitionKind::Widget, "widget", name, body).await?
                {
                    StoreOutcome::Created(name) => created.widgets.push(name),
                    StoreOutcome::Skipped(entry) => skipped.push(entry),
                }
            }

            if let Some((name, body)) = dashboard {
                match store_definition(&state, DefinitionKind::Dashboard, "dashboard", name, body)
                    .await?
                {
                    StoreOutcome::Created(name) => created.dashboard = Some(name),
                    StoreOutcome::Skipped(entry) => skipped.push(entry),
                }
            }

            Ok(Json(ChatCreatedResponse {
                reply,
                created,
                skipped,
            })
            .into_response())
        }
    }
}

enum StoreOutcome {
    Created(String),
    Skipped(Skipped),
}

/// Stores `body` under `name` unless a definition of this `kind` already
/// uses that name — an existing definition is left untouched and reported
/// as skipped rather than overwritten.
async fn store_definition(
    state: &AppState,
    kind: DefinitionKind,
    kind_label: &'static str,
    name: String,
    body: Value,
) -> Result<StoreOutcome, CanonicalError> {
    let parsed_name = DefinitionName::parse(&name).map_err(definition_error)?;

    let existing = state
        .definitions()
        .get(kind, &parsed_name)
        .await
        .map_err(definition_store_error)?;

    if existing.is_some() {
        return Ok(StoreOutcome::Skipped(Skipped {
            kind: kind_label.to_owned(),
            name,
            reason: "exists".to_owned(),
        }));
    }

    state
        .definitions()
        .put(kind, &parsed_name, &body)
        .await
        .map_err(definition_store_error)?;

    Ok(StoreOutcome::Created(name))
}

/// The tables the reader has data in, each with the field names and types
/// `TableStore::sample_fields` found in its most recent rows.
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
        DefinitionStoreError::Timeout => {
            ChatApiError::deadline_exceeded("definition store timed out").create()
        }
        DefinitionStoreError::ClickHouse(source) => {
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
