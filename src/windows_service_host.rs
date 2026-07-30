use std::path::Path;

use anyhow::Result;

use crate::cli::ServiceCommand;

#[cfg(windows)]
mod child;
#[cfg(windows)]
mod management;
#[cfg(windows)]
mod runtime;
#[cfg(windows)]
mod supervisor;

#[cfg(windows)]
const SERVICE_NAME: &str = "Latitude";
#[cfg(windows)]
const SERVICE_DISPLAY_NAME: &str = "Latitude Desktop Service";
#[cfg(windows)]
const SERVICE_DESCRIPTION: &str = "Runs Latitude at boot with separate privileged desktop and user-owned workspace hosts in the active Windows session.";
#[cfg(windows)]
const SERVICE_TYPE: windows_service::service::ServiceType =
    windows_service::service::ServiceType::OWN_PROCESS;

#[cfg(windows)]
pub fn run_command(config_path: &Path, command: &ServiceCommand) -> Result<()> {
    match command {
        ServiceCommand::Install { no_start } => management::install(config_path, !no_start),
        ServiceCommand::Uninstall => management::uninstall(),
        ServiceCommand::Start => management::start(),
        ServiceCommand::Stop => management::stop(),
        ServiceCommand::Status => management::status(),
        ServiceCommand::Run => runtime::dispatch(config_path.to_path_buf()),
    }
}

#[cfg(windows)]
pub fn dispatch(config_path: std::path::PathBuf) -> Result<()> {
    runtime::dispatch(config_path)
}

#[cfg(not(windows))]
pub fn run_command(_config_path: &Path, _command: &ServiceCommand) -> Result<()> {
    anyhow::bail!("Latitude Windows service management is only supported on Windows")
}

#[cfg(not(windows))]
pub fn dispatch(_config_path: std::path::PathBuf) -> Result<()> {
    anyhow::bail!("Latitude Windows service management is only supported on Windows")
}
