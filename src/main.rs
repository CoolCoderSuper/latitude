#![warn(unreachable_pub)]
#![warn(clippy::unused_async)]

mod cli;
mod command_protocol;
mod config;
mod desktop;
mod desktop_webrtc;
mod device;
mod http_stream;
mod internal_host;
mod project_files;
mod server;
mod state;
mod storage;
mod terminal;
mod util;
mod websocket_bridge;
mod windows_service_host;
mod workspace;

use clap::Parser;
use cli::{Cli, CliCommand, ServiceCommand};
use config::BootConfig;
use desktop::NativeSessionBridge;
use state::AppState;
use storage::CatalogStore;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "latitude=info,tower_http=info".into()),
        )
        .init();

    let cli = Cli::parse();
    if matches!(
        cli.command,
        Some(CliCommand::Service {
            command: ServiceCommand::Run
        })
    ) {
        return windows_service_host::dispatch(cli.config);
    }

    tokio::runtime::Runtime::new()?.block_on(run(cli))
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    if let Some(command) = &cli.command {
        return cli::run_command(&cli, command).await;
    }

    run_server(cli.config, cli.public_bind, cli.command_bind, None, None).await
}

async fn run_server(
    config_path: std::path::PathBuf,
    public_bind: Option<String>,
    command_bind: Option<String>,
    native_session_bridge: Option<NativeSessionBridge>,
    workspace_bridge: Option<workspace::WorkspaceBridge>,
) -> anyhow::Result<()> {
    let mut config = BootConfig::load_or_default(&config_path).await?;

    if let Some(public_bind) = public_bind {
        config.public_bind = public_bind;
    }
    if let Some(command_bind) = command_bind {
        config.command_bind = command_bind;
    }

    config.validate()?;
    let data_dir = config.resolved_data_dir(&config_path)?;
    let catalog = CatalogStore::open(data_dir).await?;

    server::run(AppState::new_with_bridges(
        config_path,
        config,
        catalog,
        native_session_bridge,
        workspace_bridge,
    ))
    .await
}
