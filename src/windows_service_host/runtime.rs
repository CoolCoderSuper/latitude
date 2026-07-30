use std::{ffi::OsString, path::PathBuf, sync::OnceLock, time::Duration};

use anyhow::{Context, Result, anyhow};
use tokio::sync::watch;
use tracing::warn;
use windows_service::{
    define_windows_service,
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus},
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

use crate::{
    desktop::NativeSessionBridge,
    workspace::{WorkspaceBridge, install_global_workspace_bridge},
};

use super::{
    SERVICE_NAME, SERVICE_TYPE,
    management::absolute_config_path,
    supervisor::{supervise_session_host, supervise_workspace_host},
};

static SERVICE_CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub(super) fn dispatch(config_path: PathBuf) -> Result<()> {
    let config_path = absolute_config_path(&config_path)?;
    SERVICE_CONFIG_PATH
        .set(config_path)
        .map_err(|_| anyhow!("Latitude service configuration was already initialized"))?;
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .context("Latitude must be started by the Windows Service Control Manager")
}

define_windows_service!(ffi_service_main, service_main);

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        warn!(%error, "Latitude Windows service failed");
    }
}

fn run_service() -> Result<()> {
    let (stop_tx, stop_rx) = watch::channel(false);
    let event_handler = move |event| -> ServiceControlHandlerResult {
        match event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = stop_tx.send(true);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate | ServiceControl::SessionChange(_) => {
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;
    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP
            | ServiceControlAccept::SHUTDOWN
            | ServiceControlAccept::SESSION_CHANGE,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::ZERO,
        process_id: None,
    })?;

    let config_path = SERVICE_CONFIG_PATH
        .get()
        .cloned()
        .ok_or_else(|| anyhow!("Latitude service config path was not initialized"))?;
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(run_service_tasks(config_path, stop_rx));
    let exit_code = if result.is_ok() {
        ServiceExitCode::Win32(0)
    } else {
        ServiceExitCode::ServiceSpecific(1)
    };
    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code,
        checkpoint: 0,
        wait_hint: Duration::ZERO,
        process_id: None,
    })?;
    result
}

async fn run_service_tasks(config_path: PathBuf, mut stop_rx: watch::Receiver<bool>) -> Result<()> {
    let desktop_bridge = NativeSessionBridge::new();
    let workspace_bridge = WorkspaceBridge::new();
    install_global_workspace_bridge(workspace_bridge.clone())?;
    let server = crate::run_server(
        config_path,
        None,
        None,
        Some(desktop_bridge.clone()),
        Some(workspace_bridge.clone()),
    );
    let desktop_supervisor_bridge = desktop_bridge.clone();
    let workspace_supervisor_bridge = workspace_bridge.clone();
    let desktop_stop_rx = stop_rx.clone();
    let workspace_stop_rx = stop_rx.clone();
    let supervisors = async move {
        tokio::try_join!(
            supervise_session_host(desktop_supervisor_bridge, desktop_stop_rx),
            supervise_workspace_host(workspace_supervisor_bridge, workspace_stop_rx),
        )
        .map(|_| ())
    };
    tokio::pin!(server);
    tokio::pin!(supervisors);

    tokio::select! {
        result = &mut server => result,
        result = &mut supervisors => result,
        changed = stop_rx.changed() => {
            if changed.is_err() || *stop_rx.borrow_and_update() {
                desktop_bridge.clear_endpoint().await;
                workspace_bridge.clear_endpoint().await;
            }
            Ok(())
        }
    }
}
