mod cli;
mod config;
mod desktop;
mod device;
mod server;
mod state;
mod storage;
mod terminal;

use clap::Parser;
use cli::Cli;
use config::LoadedConfig;
use state::AppState;
use storage::CatalogStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "latitude=info,tower_http=info".into()),
        )
        .init();

    let cli = Cli::parse();

    if let Some(command) = &cli.command {
        return cli::run_command(&cli, command).await;
    }

    let mut config = LoadedConfig::load_or_default(&cli.config).await?;

    if let Some(public_bind) = cli.public_bind {
        config.boot.public_bind = public_bind;
    }
    if let Some(command_bind) = cli.command_bind {
        config.boot.command_bind = command_bind;
    }

    config.validate()?;
    let data_dir = config.boot.resolved_data_dir(&cli.config)?;
    let catalog = CatalogStore::open(data_dir).await?;
    catalog
        .import_config_seed_if_needed(&config.catalog_seed)
        .await?;

    server::run(AppState::new(cli.config, config.boot, catalog)).await
}
