mod api;
mod catalog;
mod chat;
mod config;
mod definitions;
mod gear;
mod identity;
mod metric_query;
mod migration;
mod raw_data;
mod tables;
mod widget;

use api_gateway as _;
use authn_resolver as _;
use authz_resolver as _;
use gear_orchestrator as _;
use grpc_hub as _;
use oidc_authn_plugin as _;
use single_tenant_tr_plugin as _;
use static_authz_plugin as _;
use tenant_resolver as _;
use types_registry as _;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use toolkit::bootstrap::{AppConfig, run_server};

#[derive(Debug, Parser)]
#[command(name = "insight-v3-core")]
#[command(about = "Insight v3 Core")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run,
    Migrate,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::load_or_default(cli.config.as_ref())?;

    match cli.command.unwrap_or(Commands::Run) {
        Commands::Run => run_server(config).await,
        Commands::Migrate => {
            init_subcommand_logging();
            gear::run_migrate(&config).await
        }
    }
}

/// Plain stdout logging for `migrate`, which runs outside the bootstrap server.
fn init_subcommand_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}
