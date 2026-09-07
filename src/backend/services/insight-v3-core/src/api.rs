//! HTTP API endpoints.

use std::sync::Arc;

use axum::Router;
use toolkit::api::OpenApiRegistry;

pub(crate) mod admission;
pub(crate) mod raw_data;
pub(crate) mod tables;

use admission::IngestAdmission;

use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

#[derive(Debug)]
pub(crate) struct AppState {
    raw_data: RawDataStore,
    tables: TableStore,
}

impl AppState {
    pub(crate) fn new(raw_data: RawDataStore, tables: TableStore) -> Self {
        Self { raw_data, tables }
    }

    pub(crate) fn raw_data(&self) -> &RawDataStore {
        &self.raw_data
    }

    pub(crate) fn tables(&self) -> &TableStore {
        &self.tables
    }
}

pub(crate) fn register_routes(
    router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
    admission: IngestAdmission,
) -> Router {
    let router = tables::register_routes(router, openapi, state.clone(), admission.clone());

    raw_data::register_routes(router, openapi, state, admission)
}
