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

use crate::chat::ChatClient;
use crate::definitions::Definitions;
use crate::metric_query::MetricRunner;
use crate::raw_data::RawDataStore;
use crate::tables::TableStore;

#[derive(Debug)]
pub(crate) struct AppState {
    raw_data: RawDataStore,
    tables: TableStore,
    definitions: Arc<dyn Definitions>,
    metrics: MetricRunner,
    chat: ChatClient,
}

impl AppState {
    pub(crate) fn new(
        raw_data: RawDataStore,
        tables: TableStore,
        definitions: Arc<dyn Definitions>,
        metrics: MetricRunner,
        chat: ChatClient,
    ) -> Self {
        Self {
            raw_data,
            tables,
            definitions,
            metrics,
            chat,
        }
    }

    pub(crate) fn raw_data(&self) -> &RawDataStore {
        &self.raw_data
    }

    pub(crate) fn tables(&self) -> &TableStore {
        &self.tables
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
    let router = metric_run::register_routes(router, openapi, state.clone());

    chat::register_routes(router, openapi, state)
}
