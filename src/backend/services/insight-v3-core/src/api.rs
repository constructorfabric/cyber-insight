//! HTTP API endpoints.

use std::sync::Arc;

use axum::Router;
use toolkit::api::OpenApiRegistry;

pub(crate) mod admission;
pub(crate) mod definitions;
pub(crate) mod raw_data;
pub(crate) mod tables;

use admission::IngestAdmission;

use crate::definitions::DefinitionStore;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

#[derive(Debug)]
pub(crate) struct AppState {
    raw_data: RawDataStore,
    tables: TableStore,
    definitions: DefinitionStore,
}

impl AppState {
    pub(crate) fn new(
        raw_data: RawDataStore,
        tables: TableStore,
        definitions: DefinitionStore,
    ) -> Self {
        Self {
            raw_data,
            tables,
            definitions,
        }
    }

    pub(crate) fn raw_data(&self) -> &RawDataStore {
        &self.raw_data
    }

    pub(crate) fn tables(&self) -> &TableStore {
        &self.tables
    }

    pub(crate) fn definitions(&self) -> &DefinitionStore {
        &self.definitions
    }
}

pub(crate) fn register_routes(
    router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
    admission: IngestAdmission,
) -> Router {
    let router = tables::register_routes(router, openapi, state.clone(), admission.clone());
    let router = raw_data::register_routes(router, openapi, state.clone(), admission);

    definitions::register_routes(router, openapi, state)
}
