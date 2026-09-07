use std::fmt;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request, rejection::JsonRejection};
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json, Router};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use toolkit::api::{OpenApiRegistry, OperationBuilder};
use toolkit_canonical_errors::{CanonicalError, Http, resource_error};
use utoipa::ToSchema;

use crate::config::MAX_INGEST_TOKEN_BYTES;
use crate::raw_data::{RawDataError, RawDataRecord, RawDataStore, StoreError};

const MAX_CONCURRENT_WRITES: usize = 64;
pub(crate) const MAX_REQUEST_BODY_BYTES: usize = 1_048_576;

#[resource_error("gts.cf.insight.insight_v3_core.raw_data.v1~")]
struct RawDataApiError;

#[derive(Debug)]
pub(crate) struct AppState {
    store: RawDataStore,
}

impl AppState {
    pub(crate) fn new(store: RawDataStore) -> Self {
        Self { store }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct IngestAdmission {
    verifier: TokenVerifier,
    write_slots: Arc<tokio::sync::Semaphore>,
}

impl IngestAdmission {
    pub(crate) fn new(verifier: TokenVerifier) -> Self {
        Self {
            verifier,
            write_slots: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_WRITES)),
        }
    }
}

#[derive(Clone)]
pub(crate) struct TokenVerifier([u8; 32]);

impl TokenVerifier {
    pub(crate) fn new(token: &SecretString) -> Self {
        Self(Sha256::digest(token.expose_secret().as_bytes()).into())
    }

    fn authorizes(&self, headers: &HeaderMap) -> bool {
        if headers.get_all(AUTHORIZATION).iter().count() != 1 {
            return false;
        }
        let Some(value) = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        let Some((scheme, token)) = value.split_once(' ') else {
            return false;
        };
        if !scheme.eq_ignore_ascii_case("bearer")
            || token.is_empty()
            || token.len() > MAX_INGEST_TOKEN_BYTES
            || !token.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return false;
        }

        let actual: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        bool::from(actual.ct_eq(&self.0))
    }
}

impl fmt::Debug for TokenVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenVerifier(<redacted>)")
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct RawDataRequest {
    table: String,
    raw_data: serde_json::Value,
}

impl toolkit::api::api_dto::RequestApiDto for RawDataRequest {}

pub(crate) fn register_routes(
    host_router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
    admission: IngestAdmission,
) -> Router {
    let api = OperationBuilder::post("/v1/raw-data")
        .operation_id("insight_v3_core.raw_data.ingest")
        .summary("Store raw JSON data")
        .anonymous()
        .exposed()
        .json_request::<RawDataRequest>(openapi, "Logical table label and raw JSON data")
        .no_content_response(StatusCode::NO_CONTENT, "Raw data stored")
        .error_400(openapi)
        .error_401(openapi)
        .error_413(openapi)
        .error_415(openapi)
        .error_429(openapi)
        .error_500(openapi)
        .handler(ingest_raw_data)
        .register(Router::new(), openapi)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(middleware::from_fn_with_state(admission, authenticate))
        .layer(Extension(state));

    host_router.merge(api)
}

async fn authenticate(
    axum::extract::State(admission): axum::extract::State<IngestAdmission>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    if !admission.verifier.authorizes(&headers) {
        return no_store(unauthenticated_response());
    }
    let Ok(permit) = admission.write_slots.try_acquire_owned() else {
        return no_store(capacity_error().into_response());
    };

    // INVARIANT: admission spans body polling, preparation, and ClickHouse I/O.
    let response = next.run(request).await;
    drop(permit);

    no_store(response)
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));

    response
}

fn unauthenticated_response() -> Response {
    let mut response = CanonicalError::unauthenticated()
        .with_reason("INVALID_INGEST_TOKEN")
        .create()
        .into_response();
    response
        .headers_mut()
        .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));

    response
}

async fn ingest_raw_data(
    Extension(state): Extension<Arc<AppState>>,
    body: Result<Json<RawDataRequest>, JsonRejection>,
) -> Result<Response, CanonicalError> {
    let Json(request) = body.map_err(|error| request_rejection(&error))?;
    let record = parse_record(request).await?;

    state
        .store
        .insert(record)
        .await
        .map_err(|error| store_error(&error))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn parse_record(request: RawDataRequest) -> Result<RawDataRecord, CanonicalError> {
    let result = tokio::task::spawn_blocking(move || {
        RawDataRecord::parse(&request.table, &request.raw_data)
    })
    .await
    .map_err(|error| {
        tracing::error!(error = ?error, "raw data preparation task failed");
        internal_error()
    })?;

    result.map_err(raw_data_error)
}

fn request_rejection(error: &JsonRejection) -> CanonicalError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return RawDataApiError::invalid_argument()
            .with_field_violation("body", "Request body exceeds the limit", "TOO_LARGE")
            .with_override(Http::status_code(413))
            .create();
    }
    if error.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE {
        return RawDataApiError::invalid_argument()
            .with_field_violation("body", "Content-Type must be application/json", "INVALID")
            .with_override(Http::status_code(415))
            .create();
    }

    RawDataApiError::invalid_argument()
        .with_field_violation(
            "body",
            "Expected a JSON object containing table and raw_data",
            "INVALID",
        )
        .create()
}

fn raw_data_error(error: RawDataError) -> CanonicalError {
    match error {
        RawDataError::EmptyTableName | RawDataError::TableNameTooLong => {
            RawDataApiError::invalid_argument()
                .with_field_violation("table", error.to_string(), "INVALID")
                .create()
        }
        RawDataError::Serialization(source) => {
            tracing::error!(error = ?source, "raw data serialization failed");
            internal_error()
        }
    }
}

fn capacity_error() -> CanonicalError {
    RawDataApiError::resource_exhausted("raw data ingestion is busy")
        .with_quota_violation("raw_data_writes", "too many concurrent writes")
        .create()
}

fn store_error(error: &StoreError) -> CanonicalError {
    tracing::error!(error = ?error, "raw data insert failed");
    internal_error()
}

fn internal_error() -> CanonicalError {
    CanonicalError::internal("raw data ingestion failed").create()
}

#[cfg(test)]
mod tests;
