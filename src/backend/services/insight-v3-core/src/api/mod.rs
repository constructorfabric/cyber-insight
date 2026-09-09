//! HTTP API endpoints.

use std::sync::Arc;

use axum::Router;
use toolkit::api::OpenApiRegistry;

pub(crate) mod admission;
pub(crate) mod chat;
pub(crate) mod definitions;
pub(crate) mod metric_run;
pub(crate) mod raw_data;
pub(crate) mod tables;

use admission::IngestAdmission;

use crate::catalog::Catalog;
use crate::chat::ChatClient;
use crate::definitions::Definitions;
use crate::identity::IdentityClient;
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

/// What a refused surface says.
pub(crate) const ADMIN_ONLY: &str = "admin role required for this operation";

/// The caller's own authorization, as the gateway passed it on.
fn forwarded_authorization(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
}

/// Refuses a caller without the admin role.
///
/// The custom surfaces read and write one tenant's definitions and spend the
/// configured model budget, so they are admin-only. Roles live in the identity
/// service, so the caller's authorization is forwarded there.
///
/// An identity that is absent or unreachable is a server error, never a
/// permit: a role check that cannot be made has not passed.
pub(crate) async fn require_admin(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    denied: fn() -> toolkit_canonical_errors::CanonicalError,
) -> Result<(), toolkit_canonical_errors::CanonicalError> {
    if !state.identity.is_configured() {
        tracing::error!("identity service is not configured; admin access cannot be verified");
        return Err(toolkit_canonical_errors::CanonicalError::internal(
            "failed to verify caller permissions",
        )
        .create());
    }

    let is_admin = state
        .identity
        .is_admin(forwarded_authorization(headers))
        .await
        .map_err(|error| {
            if error.is_about_the_caller() {
                tracing::warn!(error = %error, "the caller could not be identified");
                return denied();
            }
            tracing::error!(error = %error, "admin role check failed");
            toolkit_canonical_errors::CanonicalError::internal(
                "failed to verify caller permissions",
            )
            .create()
        })?;

    if is_admin {
        return Ok(());
    }

    Err(denied())
}

#[derive(Debug)]
pub(crate) struct AppState {
    raw_data: RawDataStore,
    tables: TableStore,
    definitions: Arc<dyn Definitions>,
    metrics: MetricRunner,
    chat: ChatClient,
    identity: IdentityClient,
    catalog: Catalog,
}

impl AppState {
    pub(crate) fn new(
        raw_data: RawDataStore,
        tables: TableStore,
        definitions: Arc<dyn Definitions>,
        metrics: MetricRunner,
        chat: ChatClient,
        identity: IdentityClient,
        catalog: Catalog,
    ) -> Self {
        Self {
            raw_data,
            tables,
            definitions,
            metrics,
            chat,
            identity,
            catalog,
        }
    }

    pub(crate) fn raw_data(&self) -> &RawDataStore {
        &self.raw_data
    }

    pub(crate) fn tables(&self) -> &TableStore {
        &self.tables
    }

    pub(crate) fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub(crate) fn definitions(&self) -> &dyn Definitions {
        self.definitions.as_ref()
    }

    pub(crate) fn metrics(&self) -> &MetricRunner {
        &self.metrics
    }

    pub(crate) fn chat(&self) -> &ChatClient {
        &self.chat
    }

    pub(crate) fn surfaces(&self) -> crate::custom::Surfaces<'_> {
        crate::custom::Surfaces::new(self.definitions.as_ref(), &self.metrics, &self.catalog)
    }
}

pub(crate) fn register_routes(
    router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
    admission: IngestAdmission,
) -> Router {
    // Built apart from the host's router so the layer wraps this service's
    // own routes, and merged in after — the host's own endpoints carry
    // their own context.
    let api = tables::register_routes(Router::new(), openapi, state.clone(), admission.clone());
    let api = raw_data::register_routes(api, openapi, state.clone(), admission);
    let api = definitions::register_routes(api, openapi, state.clone());
    let api = metric_run::register_routes(api, openapi, state.clone());
    let api = chat::register_routes(api, openapi, state)
        .layer(insight_log_context::LogContextLayer::new());

    router.merge(api)
}

#[cfg(test)]
mod log_context_tests;
#[cfg(test)]
mod log_leak_tests;
