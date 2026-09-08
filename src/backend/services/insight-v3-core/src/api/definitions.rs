//! Metric, widget and dashboard definition HTTP endpoints.

use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use toolkit::api::{OpenApiRegistry, OperationBuilder, ParamLocation, ParamSpec};
use toolkit_canonical_errors::{CanonicalError, resource_error};
use utoipa::ToSchema;

use super::AppState;
use crate::definitions::{
    Change, DefinitionError, DefinitionKind, DefinitionName, DefinitionStoreError,
};

#[derive(Debug, Deserialize, ToSchema)]
struct RenameRequest {
    /// The name it should answer to from now on.
    to: String,
}

impl toolkit::api::api_dto::RequestApiDto for RenameRequest {}

#[derive(Debug, Serialize, ToSchema)]
struct RenameResponse {
    name: String,
    /// The definitions that pointed at the old name and now point here.
    rewritten: Vec<String>,
}

#[resource_error("gts.cf.insight.insight_v3_core.definitions.v1~")]
struct DefinitionApiError;

// `.anonymous()`: these routes trust the gateway to authenticate the
// `__Host-sid` session cookie before forwarding. Must stay off the network
// (see docker-compose.yml's loopback port binding).
pub(crate) fn register_routes(
    router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
) -> Router {
    let router = register_kind(
        router,
        openapi,
        state.clone(),
        DefinitionKind::Metric,
        "metrics",
    );
    let router = register_kind(
        router,
        openapi,
        state.clone(),
        DefinitionKind::Widget,
        "widgets",
    );

    register_kind(
        router,
        openapi,
        state,
        DefinitionKind::Dashboard,
        "dashboards",
    )
}

fn register_kind(
    host_router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
    kind: DefinitionKind,
    segment: &str,
) -> Router {
    let name_param = ParamSpec {
        name: "name".to_owned(),
        location: ParamLocation::Path,
        required: true,
        description: Some("Definition name".to_owned()),
        param_type: "string".to_owned(),
        array: false,
    };

    let delete_param = name_param.clone();
    let rename_param = name_param.clone();

    let put = OperationBuilder::put(format!("/v1/{segment}/{{name}}"))
        .operation_id(format!("insight_v3_core.{segment}.put"))
        .summary("Create or replace a definition")
        .anonymous()
        .exposed()
        .param(name_param.clone())
        .json_request::<serde_json::Value>(openapi, "The definition body")
        .no_content_response(StatusCode::NO_CONTENT, "Definition stored")
        .error_400(openapi)
        .error_500(openapi)
        .error_504(openapi)
        .handler(put_definition)
        .register(Router::new(), openapi)
        .layer(Extension(state.clone()))
        .layer(Extension(kind));

    let get = OperationBuilder::get(format!("/v1/{segment}/{{name}}"))
        .operation_id(format!("insight_v3_core.{segment}.get"))
        .summary("Read a definition")
        .anonymous()
        .exposed()
        .param(name_param)
        .json_response(StatusCode::OK, "The definition body")
        .error_400(openapi)
        .error_404(openapi)
        .error_500(openapi)
        .error_504(openapi)
        .handler(get_definition)
        .register(Router::new(), openapi)
        .layer(Extension(state.clone()))
        .layer(Extension(kind));

    let list = OperationBuilder::get(format!("/v1/{segment}"))
        .operation_id(format!("insight_v3_core.{segment}.list"))
        .summary("List definition names")
        .anonymous()
        .exposed()
        .json_response(StatusCode::OK, "Definition names")
        .error_500(openapi)
        .error_504(openapi)
        .handler(list_definitions)
        .register(Router::new(), openapi)
        .layer(Extension(state.clone()))
        .layer(Extension(kind));

    let remove = OperationBuilder::delete(format!("/v1/{segment}/{{name}}"))
        .operation_id(format!("insight_v3_core.{segment}.delete"))
        .summary("Remove a definition")
        .anonymous()
        .exposed()
        .param(delete_param)
        .no_content_response(StatusCode::NO_CONTENT, "Definition removed")
        .error_400(openapi)
        .error_404(openapi)
        .error_500(openapi)
        .error_504(openapi)
        .handler(delete_definition)
        .register(Router::new(), openapi)
        .layer(Extension(state.clone()))
        .layer(Extension(kind));

    let rename = OperationBuilder::post(format!("/v1/{segment}/{{name}}/rename"))
        .operation_id(format!("insight_v3_core.{segment}.rename"))
        .summary("Rename a definition, and everything that points at it")
        .anonymous()
        .exposed()
        .param(rename_param)
        .json_request::<RenameRequest>(openapi, "The new name")
        .json_response(StatusCode::OK, "The new name, and what was rewritten")
        .error_400(openapi)
        .error_404(openapi)
        .error_409(openapi)
        .error_500(openapi)
        .error_504(openapi)
        .handler(rename_definition)
        .register(Router::new(), openapi)
        .layer(Extension(state))
        .layer(Extension(kind));

    host_router
        .merge(put)
        .merge(get)
        .merge(list)
        .merge(remove)
        .merge(rename)
}

/// What still draws the definition the caller is removing.
///
/// A widget whose metric is gone renders an error where a chart should be, and
/// a dashboard holding a widget that is gone renders a gap. Both were the
/// failure the reader could not diagnose, so a definition in use is kept and
/// the dependents named.
async fn dependents_of(
    state: &AppState,
    kind: DefinitionKind,
    name: &DefinitionName,
) -> Result<Vec<String>, CanonicalError> {
    let Some((holder, needle)) = held_by(kind) else {
        return Ok(Vec::new());
    };

    let mut used_by = Vec::new();
    for holder_name in state
        .definitions()
        .list(holder)
        .await
        .map_err(definition_store_error)?
    {
        let Ok(parsed) = DefinitionName::parse(&holder_name) else {
            continue;
        };
        let Some(body) = state
            .definitions()
            .get(holder, &parsed)
            .await
            .map_err(definition_store_error)?
        else {
            continue;
        };

        let names = match body.get(needle) {
            Some(serde_json::Value::String(one)) => vec![one.as_str()],
            Some(serde_json::Value::Array(many)) => {
                many.iter().filter_map(serde_json::Value::as_str).collect()
            }
            _ => Vec::new(),
        };
        if names.contains(&name.as_str()) {
            used_by.push(holder_name);
        }
    }

    Ok(used_by)
}

/// Which kind names this one, and under which field.
///
/// A widget names its metric; a dashboard names its widgets. Nothing names a
/// dashboard.
fn held_by(kind: DefinitionKind) -> Option<(DefinitionKind, &'static str)> {
    match kind {
        DefinitionKind::Metric => Some((DefinitionKind::Widget, "metric")),
        DefinitionKind::Widget => Some((DefinitionKind::Dashboard, "widgets")),
        DefinitionKind::Dashboard => None,
    }
}

/// The same body, pointed at the new name.
fn pointed_at(mut body: serde_json::Value, field: &str, from: &str, to: &str) -> serde_json::Value {
    match body.get_mut(field) {
        Some(serde_json::Value::String(one)) if one == from => to.clone_into(one),
        Some(serde_json::Value::Array(many)) => {
            for entry in many {
                if entry.as_str() == Some(from) {
                    *entry = serde_json::Value::String(to.to_owned());
                }
            }
        }
        _ => {}
    }

    body
}

/// Renames a definition, and rewrites whatever drew it under the old name.
///
/// A name is the only handle a widget has on its metric, and a dashboard on
/// its widgets, so renaming one alone would break the others - the same
/// broken chart the widget check exists to prevent. The new name, the removal
/// of the old, and every rewritten dependent are one transaction.
async fn rename_definition(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    Json(request): Json<RenameRequest>,
) -> Result<Response, CanonicalError> {
    crate::api::require_admin(&state, &headers, || {
        DefinitionApiError::permission_denied()
            .with_reason(crate::api::ADMIN_ONLY)
            .create()
    })
    .await?;

    let from = DefinitionName::parse(&name).map_err(definition_error)?;
    let to = DefinitionName::parse(&request.to).map_err(definition_error)?;

    let body = state
        .definitions()
        .get(kind, &from)
        .await
        .map_err(definition_store_error)?
        .ok_or_else(|| {
            DefinitionApiError::not_found(format!("`{}` was not found", from.as_str()))
                .with_resource(from.as_str())
                .create()
        })?;

    if to == from {
        return Ok(Json(RenameResponse {
            name: to.as_str().to_owned(),
            rewritten: Vec::new(),
        })
        .into_response());
    }

    if state
        .definitions()
        .get(kind, &to)
        .await
        .map_err(definition_store_error)?
        .is_some()
    {
        return Err(DefinitionApiError::already_exists(format!(
            "`{}` is already taken",
            to.as_str()
        ))
        .with_resource(to.as_str())
        .create());
    }

    let mut changes = vec![
        Change::Put(kind, to.clone(), body),
        Change::Delete(kind, from.clone()),
    ];
    let mut rewritten = Vec::new();
    if let Some((holder, field)) = held_by(kind) {
        for holder_name in dependents_of(&state, kind, &from).await? {
            let parsed = DefinitionName::parse(&holder_name).map_err(definition_error)?;
            let Some(body) = state
                .definitions()
                .get(holder, &parsed)
                .await
                .map_err(definition_store_error)?
            else {
                continue;
            };

            changes.push(Change::Put(
                holder,
                parsed,
                pointed_at(body, field, from.as_str(), to.as_str()),
            ));
            rewritten.push(holder_name);
        }
    }

    state
        .definitions()
        .apply(&changes)
        .await
        .map_err(definition_store_error)?;

    Ok(Json(RenameResponse {
        name: to.as_str().to_owned(),
        rewritten,
    })
    .into_response())
}

async fn delete_definition(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, CanonicalError> {
    crate::api::require_admin(&state, &headers, || {
        DefinitionApiError::permission_denied()
            .with_reason(crate::api::ADMIN_ONLY)
            .create()
    })
    .await?;

    let name = DefinitionName::parse(&name).map_err(definition_error)?;

    let used_by = dependents_of(&state, kind, &name).await?;
    if !used_by.is_empty() {
        return Err(DefinitionApiError::failed_precondition()
            .with_precondition_violation(
                "name",
                format!("still in use by {}", used_by.join(", ")),
                "in_use",
            )
            .create());
    }

    let removed = state
        .definitions()
        .delete(kind, &name)
        .await
        .map_err(definition_store_error)?;

    Ok(if removed {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    })
}

async fn put_definition(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, CanonicalError> {
    crate::api::require_admin(&state, &headers, || {
        DefinitionApiError::permission_denied()
            .with_reason(crate::api::ADMIN_ONLY)
            .create()
    })
    .await?;

    let name = DefinitionName::parse(&name).map_err(definition_error)?;

    if kind == DefinitionKind::Widget {
        check_widget(&state, &body).await?;
    }

    state
        .definitions()
        .put(kind, &name, &body)
        .await
        .map_err(definition_store_error)?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Refuses a widget whose metric cannot supply the columns it draws.
///
/// The chart used to render its axes and nothing else, which reads as missing
/// data rather than as a definition naming a column that is not there.
pub(crate) async fn check_widget(
    state: &AppState,
    body: &serde_json::Value,
) -> Result<(), CanonicalError> {
    let widget: crate::widget::Widget =
        serde_json::from_value(body.clone()).map_err(|error| widget_error(&error.into()))?;

    let metric_name = DefinitionName::parse(widget.metric()).map_err(definition_error)?;
    let stored = state
        .definitions()
        .get(DefinitionKind::Metric, &metric_name)
        .await
        .map_err(definition_store_error)?
        .ok_or_else(|| {
            widget_error(&crate::widget::WidgetError::NoMetric(
                widget.metric().to_owned(),
            ))
        })?;

    let metric: crate::metric_query::MetricQuery =
        serde_json::from_value(stored).map_err(|error| widget_error(&error.into()))?;

    widget
        .check_against(&metric)
        .map_err(|error| widget_error(&error))
}

pub(crate) fn widget_error(error: &crate::widget::WidgetError) -> CanonicalError {
    DefinitionApiError::invalid_argument()
        .with_field_violation("body", error.to_string(), "INVALID")
        .create()
}

async fn get_definition(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, CanonicalError> {
    crate::api::require_admin(&state, &headers, || {
        DefinitionApiError::permission_denied()
            .with_reason(crate::api::ADMIN_ONLY)
            .create()
    })
    .await?;

    let name = DefinitionName::parse(&name).map_err(definition_error)?;

    let body = state
        .definitions()
        .get(kind, &name)
        .await
        .map_err(definition_store_error)?;

    Ok(match body {
        Some(body) => Json(body).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

async fn list_definitions(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
    headers: axum::http::HeaderMap,
) -> Result<Response, CanonicalError> {
    crate::api::require_admin(&state, &headers, || {
        DefinitionApiError::permission_denied()
            .with_reason(crate::api::ADMIN_ONLY)
            .create()
    })
    .await?;

    let names = state
        .definitions()
        .list(kind)
        .await
        .map_err(definition_store_error)?;

    Ok(Json(serde_json::json!({ "names": names })).into_response())
}

fn definition_error(error: DefinitionError) -> CanonicalError {
    DefinitionApiError::invalid_argument()
        .with_field_violation("name", error.to_string(), "INVALID")
        .create()
}

fn definition_store_error(error: DefinitionStoreError) -> CanonicalError {
    match error {
        // Waiting for a connection is the store being busy, not broken.
        DefinitionStoreError::Database(sea_orm::DbErr::ConnectionAcquire(source)) => {
            tracing::warn!(error = ?source, "definition store connection timed out");
            DefinitionApiError::deadline_exceeded("definition store timed out").create()
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
