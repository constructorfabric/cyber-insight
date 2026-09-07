//! HTTP API endpoints.

use std::sync::Arc;

use axum::Router;
use toolkit::api::OpenApiRegistry;

pub(crate) mod admission;
pub(crate) mod definitions;
pub(crate) mod metric_run;
pub(crate) mod raw_data;
pub(crate) mod tables;

use admission::IngestAdmission;

use crate::definitions::DefinitionStore;
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

#[derive(Debug)]
pub(crate) struct AppState {
    raw_data: RawDataStore,
    tables: TableStore,
    definitions: DefinitionStore,
    metrics: MetricRunner,
}

impl AppState {
    pub(crate) fn new(
        raw_data: RawDataStore,
        tables: TableStore,
        definitions: DefinitionStore,
        metrics: MetricRunner,
    ) -> Self {
        Self {
            raw_data,
            tables,
            definitions,
            metrics,
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

    pub(crate) fn metrics(&self) -> &MetricRunner {
        &self.metrics
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
    let router = definitions::register_routes(router, openapi, state.clone());

    metric_run::register_routes(router, openapi, state)
}
