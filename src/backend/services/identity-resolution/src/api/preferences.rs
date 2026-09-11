//! The caller's own settings — `GET`/`PUT /v1/me/preferences`.
//!
//! Not admin-gated: a preference belongs to the person who signed in, and the
//! owner is taken from the verified request context rather than from the body,
//! so no caller can write another person's setting. A person who never chose
//! reads the default.

use std::sync::Arc;

use axum::Json;
use axum::extract::Extension;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use toolkit_canonical_errors::CanonicalError;
use toolkit_security::SecurityContext;
use utoipa::ToSchema;

use super::AppState;
use super::canonical_json::CanonicalJson;
use super::error::PreferenceError;
use super::gate::{require_caller, require_person};
use crate::domain::timezone::Timezone;
use crate::infra::db::preferences_repo;

/// The settings this person chose, with the default filled in for whatever
/// they have not.
#[derive(Debug, Serialize, ToSchema)]
pub struct PreferencesResponse {
    /// The IANA zone their dashboard days are cut on.
    pub timezone: String,
}
impl toolkit::api::api_dto::ResponseApiDto for PreferencesResponse {}

/// A change to those settings. An unknown field is refused, not dropped.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PreferencesRequest {
    pub timezone: String,
}

/// `GET /v1/me/preferences` — what the caller chose (any signed-in user).
///
/// # Errors
///
/// 401 when the gateway JWT carries no person subject; 500 when the store
/// cannot be read.
pub async fn get_preferences(
    Extension(state): Extension<Arc<AppState>>,
    Extension(ctx): Extension<SecurityContext>,
) -> Result<impl IntoResponse, CanonicalError> {
    let caller = require_caller(&ctx)?;

    let chosen = preferences_repo::timezone_of(&state.db, ctx.subject_tenant_id(), caller)
        .await
        .map_err(read_err)?;

    Ok(Json(response(&chosen.unwrap_or_default())))
}

/// `PUT /v1/me/preferences` — record the caller's own settings.
///
/// # Errors
///
/// 401 when the gateway JWT carries no person subject; 403 for a service
/// principal, which has no preferences of its own; 400 when the timezone is
/// not an IANA name; 500 when the store cannot be written.
pub async fn put_preferences(
    Extension(state): Extension<Arc<AppState>>,
    Extension(ctx): Extension<SecurityContext>,
    CanonicalJson(request): CanonicalJson<PreferencesRequest>,
) -> Result<impl IntoResponse, CanonicalError> {
    let caller = require_person(&ctx)?;

    let timezone = Timezone::parse(&request.timezone).map_err(|error| {
        PreferenceError::invalid_argument()
            .with_field_violation("timezone", error.to_string(), "INVALID")
            .create()
    })?;

    preferences_repo::set_timezone(&state.db, ctx.subject_tenant_id(), caller, &timezone)
        .await
        .map_err(write_err)?;

    Ok(Json(response(&timezone)))
}

fn response(timezone: &Timezone) -> PreferencesResponse {
    PreferencesResponse {
        timezone: timezone.as_str().to_owned(),
    }
}

#[expect(clippy::needless_pass_by_value, reason = "used directly as map_err")]
fn read_err(e: anyhow::Error) -> CanonicalError {
    tracing::error!(error = %e, "caller preference read failed");
    CanonicalError::internal("failed to read caller preferences").create()
}

#[expect(clippy::needless_pass_by_value, reason = "used directly as map_err")]
fn write_err(e: anyhow::Error) -> CanonicalError {
    tracing::error!(error = %e, "caller preference write failed");
    CanonicalError::internal("failed to save caller preferences").create()
}
