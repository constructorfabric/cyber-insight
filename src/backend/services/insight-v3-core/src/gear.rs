use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use toolkit::api::OpenApiRegistry;
use toolkit::{Gear, GearCtx, RestApiCapability};

#[toolkit::gear(name = "insight-v3-core", capabilities = [rest])]
pub struct InsightV3CoreGear {
    runtime: OnceLock<RuntimeState>,
}

impl Default for InsightV3CoreGear {
    fn default() -> Self {
        Self {
            runtime: OnceLock::new(),
        }
    }
}

impl std::fmt::Debug for InsightV3CoreGear {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InsightV3CoreGear")
            .field("initialized", &self.runtime.get().is_some())
            .finish()
    }
}

#[derive(Debug)]
struct RuntimeState {
    app: Arc<crate::api::AppState>,
    admission: crate::api::IngestAdmission,
}

#[async_trait]
impl Gear for InsightV3CoreGear {
    async fn init(&self, ctx: &GearCtx) -> anyhow::Result<()> {
        let config: crate::config::GearConfig = ctx.config()?;
        let config = config.validate()?;
        let token_verifier = crate::api::TokenVerifier::new(config.ingest_token());
        let admission = crate::api::IngestAdmission::new(token_verifier);
        let store = crate::raw_data::RawDataStore::new(config.clickhouse_client());
        let runtime = RuntimeState {
            app: Arc::new(crate::api::AppState::new(store)),
            admission,
        };
        self.runtime
            .set(runtime)
            .map_err(|_| anyhow::anyhow!("{} gear already initialized", Self::MODULE_NAME))?;

        Ok(())
    }
}

impl RestApiCapability for InsightV3CoreGear {
    fn register_rest(
        &self,
        _ctx: &GearCtx,
        router: axum::Router,
        openapi: &dyn OpenApiRegistry,
    ) -> anyhow::Result<axum::Router> {
        let runtime = self
            .runtime
            .get()
            .ok_or_else(|| anyhow::anyhow!("insight-v3-core gear not initialized"))?;

        Ok(crate::api::register_routes(
            router,
            openapi,
            runtime.app.clone(),
            runtime.admission.clone(),
        ))
    }
}

pub(crate) async fn run_migrate(app: &toolkit::bootstrap::AppConfig) -> anyhow::Result<()> {
    let config = crate::config::ValidatedConfig::from_app_config(app)?;
    let client = config.clickhouse_client();

    crate::migration::migrate(&client).await?;
    tracing::info!("raw_data migration complete");

    Ok(())
}
