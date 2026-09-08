//! Metric, widget and dashboard definition HTTP endpoints.

use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use toolkit::api::{OpenApiRegistry, OperationBuilder, ParamLocation, ParamSpec};
use toolkit_canonical_errors::{CanonicalError, resource_error};

use super::AppState;
use crate::definitions::{DefinitionError, DefinitionKind, DefinitionName, DefinitionStoreError};

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
        .layer(Extension(state))
        .layer(Extension(kind));

    host_router.merge(put).merge(get).merge(list)
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
