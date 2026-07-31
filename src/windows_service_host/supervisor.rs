use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, Result, anyhow};
use rand::random;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::{desktop::NativeSessionBridge, util::encode_hex, workspace::WorkspaceBridge};

use super::child::{
    NO_CONSOLE_SESSION, SessionHostProcess, WorkspaceHostProcess, active_interactive_session_id,
    reserve_loopback_address,
};

pub(super) async fn supervise_session_host(
    bridge: NativeSessionBridge,
    stop_rx: watch::Receiver<bool>,
) -> Result<()> {
    supervise_host(Host::Desktop(bridge), stop_rx).await
}

pub(super) async fn supervise_workspace_host(
    bridge: WorkspaceBridge,
    stop_rx: watch::Receiver<bool>,
) -> Result<()> {
    supervise_host(Host::Workspace(bridge), stop_rx).await
}

enum Host {
    Desktop(NativeSessionBridge),
    Workspace(WorkspaceBridge),
}

enum HostProcess {
    Desktop(SessionHostProcess),
    Workspace(WorkspaceHostProcess),
}

async fn supervise_host(host: Host, mut stop_rx: watch::Receiver<bool>) -> Result<()> {
    let executable =
        std::env::current_exe().context("Latitude service executable path is unavailable")?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()?;
    let mut child: Option<HostProcess> = None;
    let mut retry_after = tokio::time::Instant::now();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow_and_update() {
                    break;
                }
            }
            _ = interval.tick() => {}
        }

        let session_id = active_interactive_session_id();
        let child_is_stale = child
            .as_ref()
            .is_some_and(|running| running.session_id() != session_id || running.has_exited());
        if child_is_stale {
            host.clear().await;
            child.take();
        }

        if session_id == NO_CONSOLE_SESSION {
            host.clear().await;
            child.take();
            continue;
        }
        if child.is_some() || tokio::time::Instant::now() < retry_after {
            continue;
        }

        let address = reserve_loopback_address()?;
        let token = encode_hex(random::<[u8; 32]>());
        match host.spawn(&executable, session_id, address, &token) {
            Ok(process) => match host.connect(&client, address, token, &process).await {
                Ok(()) => {
                    info!(host = host.name(), session_id, bind = %address, "host is ready");
                    child = Some(process);
                }
                Err(error) => {
                    warn!(host = host.name(), session_id, %error, "host did not become ready");
                    retry_after = tokio::time::Instant::now() + Duration::from_secs(2);
                }
            },
            Err(error) => {
                warn!(host = host.name(), session_id, %error, "host could not be started");
                retry_after = tokio::time::Instant::now() + Duration::from_secs(2);
            }
        }
    }

    host.clear().await;
    drop(child);
    Ok(())
}

impl Host {
    fn name(&self) -> &'static str {
        match self {
            Self::Desktop(_) => "native desktop session",
            Self::Workspace(_) => "user workspace",
        }
    }

    fn command(&self) -> &'static str {
        match self {
            Self::Desktop(_) => "session-host",
            Self::Workspace(_) => "workspace-host",
        }
    }

    fn spawn(
        &self,
        executable: &std::path::Path,
        session_id: u32,
        address: SocketAddr,
        token: &str,
    ) -> Result<HostProcess> {
        match self {
            Self::Desktop(_) => SessionHostProcess::spawn(executable, session_id, address, token)
                .map(HostProcess::Desktop),
            Self::Workspace(_) => {
                WorkspaceHostProcess::spawn(executable, session_id, address, token)
                    .map(HostProcess::Workspace)
            }
        }
    }

    async fn connect(
        &self,
        client: &reqwest::Client,
        address: SocketAddr,
        token: String,
        process: &HostProcess,
    ) -> Result<()> {
        let response = wait_for_host(client, address, &token, process, self.command()).await?;
        match (self, process) {
            (Self::Desktop(bridge), HostProcess::Desktop(process)) => {
                let _ = process;
                bridge.set_endpoint(address, token).await;
                Ok(())
            }
            (Self::Workspace(bridge), HostProcess::Workspace(process)) => {
                let _ = process;
                let health = response
                    .json()
                    .await
                    .context("workspace-host health response was invalid")?;
                bridge.set_endpoint(address, token, health).await;
                Ok(())
            }
            _ => unreachable!("host spawned the wrong process type"),
        }
    }

    async fn clear(&self) {
        match self {
            Self::Desktop(bridge) => bridge.clear_endpoint().await,
            Self::Workspace(bridge) => bridge.clear_endpoint().await,
        }
    }
}

impl HostProcess {
    fn session_id(&self) -> u32 {
        match self {
            Self::Desktop(process) => process.session_id,
            Self::Workspace(process) => process.session_id,
        }
    }

    fn has_exited(&self) -> bool {
        match self {
            Self::Desktop(process) => process.has_exited(),
            Self::Workspace(process) => process.has_exited(),
        }
    }
}

async fn wait_for_host(
    client: &reqwest::Client,
    address: SocketAddr,
    token: &str,
    process: &HostProcess,
    command: &str,
) -> Result<reqwest::Response> {
    let url = format!("http://{address}/health");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if process.has_exited() {
            return Err(anyhow!("{command} process exited during startup"));
        }
        if let Ok(response) = client.get(&url).bearer_auth(token).send().await
            && response.status().is_success()
        {
            return Ok(response);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out waiting for the {command} health endpoint"
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
