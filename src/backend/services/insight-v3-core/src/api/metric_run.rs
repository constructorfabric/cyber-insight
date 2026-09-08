//! Compile-and-run endpoint for stored metric definitions.

use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use toolkit::api::{OpenApiRegistry, OperationBuilder, ParamLocation, ParamSpec};
use toolkit_canonical_errors::{CanonicalError, resource_error};

use super::AppState;
use crate::custom::CustomError;
use crate::definitions::{DefinitionError, DefinitionName, DefinitionStoreError};
use crate::metric_query::{MetricQueryError, MetricRunError};

#[resource_error("gts.cf.insight.insight_v3_core.metric_run.v1~")]
struct MetricRunApiError;

pub(crate) fn register_routes(
    router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
) -> Router {
    let name_param = ParamSpec {
        name: "name".to_owned(),
        location: ParamLocation::Path,
        required: true,
        description: Some("Metric name".to_owned()),
        param_type: "string".to_owned(),
        array: false,
    };

    let run = OperationBuilder::post("/v1/metrics/{name}/run")
        .operation_id("insight_v3_core.metrics.run")
        .summary("Compile and run a metric")
        .anonymous()
        .exposed()
        .param(name_param)
        .json_response(StatusCode::OK, "Query result")
        .error_400(openapi)
        .error_404(openapi)
        .error_500(openapi)
        .error_504(openapi)
        .handler(run_metric)
        .register(Router::new(), openapi)
        .layer(Extension(state));

    router.merge(run)
}

async fn run_metric(
    Extension(state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, CanonicalError> {
    crate::api::require_admin(&state, &headers, || {
        MetricRunApiError::permission_denied()
            .with_reason(crate::api::ADMIN_ONLY)
            .create()
    })
    .await?;

    let name = DefinitionName::parse(&name).map_err(definition_error)?;

    let result = state
        .surfaces()
        .run_metric(&name)
        .await
        .map_err(custom_error)?;

    Ok(Json(result).into_response())
}

fn definition_error(error: DefinitionError) -> CanonicalError {
    MetricRunApiError::invalid_argument()
        .with_field_violation("name", error.to_string(), "INVALID")
        .create()
}

fn custom_error(error: CustomError) -> CanonicalError {
    match error {
        CustomError::NotFound { name, .. } => metric_not_found(&name),
        CustomError::Body(source) => invalid_metric_body(&source),
        CustomError::Compile(source) => compile_error(&source),
        CustomError::Run(source) => run_error(source),
        CustomError::Store(source) => definition_store_error(source),
        CustomError::InUse { .. } | CustomError::Widget(_) | CustomError::Catalog(_) => {
            tracing::error!(%error, "running a metric produced an unrelated failure");
            CanonicalError::internal("metric query execution failed").create()
        }
    }
}

fn metric_not_found(name: &str) -> CanonicalError {
    MetricRunApiError::not_found(format!("metric `{name}` was not found"))
        .with_resource(name)
        .create()
}

fn invalid_metric_body(error: &serde_json::Error) -> CanonicalError {
    MetricRunApiError::invalid_argument()
        .with_field_violation("body", error.to_string(), "INVALID")
        .create()
}

fn compile_error(error: &MetricQueryError) -> CanonicalError {
    MetricRunApiError::invalid_argument()
        .with_field_violation("body", error.to_string(), "INVALID")
        .create()
}

fn definition_store_error(error: DefinitionStoreError) -> CanonicalError {
    match error {
        // Waiting for a connection is the store being busy, not broken.
        DefinitionStoreError::Database(sea_orm::DbErr::ConnectionAcquire(source)) => {
            tracing::warn!(error = ?source, "definition store connection timed out");
            MetricRunApiError::deadline_exceeded("definition store timed out").create()
        }
        DefinitionStoreError::Database(source) => {
            tracing::error!(error = ?source, "metric definition lookup failed");
            CanonicalError::internal("definition store operation failed").create()
        }
        DefinitionStoreError::Json(source) => {
            tracing::error!(error = ?source, "metric definition body deserialization failed");
            CanonicalError::internal("definition store operation failed").create()
        }
    }
}

fn run_error(error: MetricRunError) -> CanonicalError {
    match error {
        MetricRunError::Timeout => {
            MetricRunApiError::deadline_exceeded("metric query timed out").create()
        }
        MetricRunError::ResultTooLarge => MetricRunApiError::invalid_argument()
            .with_field_violation("body", "metric result exceeded the size limit", "TOO_LARGE")
            .create(),
        MetricRunError::ClickHouse(source) => {
            tracing::error!(error = ?source, "metric query execution failed");
            CanonicalError::internal("metric query execution failed").create()
        }
        MetricRunError::InvalidResponse(source) => {
            tracing::error!(error = ?source, "metric query result deserialization failed");
            CanonicalError::internal("metric query execution failed").create()
        }
    }
}

#[cfg(test)]
mod tests;
