use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, Result, anyhow};
use rand::random;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::{
    desktop::NativeSessionBridge,
    workspace::{WorkspaceBridge, WorkspaceHealth},
};

use super::child::{
    NO_CONSOLE_SESSION, SessionHostProcess, WorkspaceHostProcess, active_interactive_session_id,
    encode_hex, reserve_loopback_address,
};

pub(super) async fn supervise_session_host(
    bridge: NativeSessionBridge,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<()> {
    let executable =
        std::env::current_exe().context("Latitude service executable path is unavailable")?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()?;
    let mut child: Option<SessionHostProcess> = None;
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
            .is_some_and(|running| running.session_id != session_id || running.has_exited());
        if child_is_stale {
            bridge.clear_endpoint().await;
            child.take();
        }

        if session_id == NO_CONSOLE_SESSION {
            bridge.clear_endpoint().await;
            child.take();
            continue;
        }
        if child.is_some() || tokio::time::Instant::now() < retry_after {
            continue;
        }

        let address = reserve_loopback_address()?;
        let token = encode_hex(random::<[u8; 32]>());
        match SessionHostProcess::spawn(&executable, session_id, address, &token) {
            Ok(process) => match wait_for_session_host(&client, address, &token, &process).await {
                Ok(()) => {
                    info!(
                        session_id,
                        bind = %address,
                        "native desktop session host is ready"
                    );
                    bridge.set_endpoint(address, token).await;
                    child = Some(process);
                }
                Err(error) => {
                    warn!(session_id, %error, "native desktop session host did not become ready");
                    retry_after = tokio::time::Instant::now() + Duration::from_secs(2);
                }
            },
            Err(error) => {
                warn!(session_id, %error, "native desktop session host could not be started");
                retry_after = tokio::time::Instant::now() + Duration::from_secs(2);
            }
        }
    }

    bridge.clear_endpoint().await;
    drop(child);
    Ok(())
}

pub(super) async fn supervise_workspace_host(
    bridge: WorkspaceBridge,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<()> {
    let executable =
        std::env::current_exe().context("Latitude service executable path is unavailable")?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()?;
    let mut child: Option<WorkspaceHostProcess> = None;
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
            .is_some_and(|running| running.session_id != session_id || running.has_exited());
        if child_is_stale {
            bridge.clear_endpoint().await;
            child.take();
        }

        if session_id == NO_CONSOLE_SESSION {
            bridge.clear_endpoint().await;
            child.take();
            continue;
        }
        if child.is_some() || tokio::time::Instant::now() < retry_after {
            continue;
        }

        let address = reserve_loopback_address()?;
        let token = encode_hex(random::<[u8; 32]>());
        match WorkspaceHostProcess::spawn(&executable, session_id, address, &token) {
            Ok(process) => {
                match wait_for_workspace_host(&client, address, &token, &process).await {
                    Ok(health) => {
                        info!(
                            session_id,
                            bind = %address,
                            identity = %health.identity,
                            "user workspace host is ready"
                        );
                        bridge.set_endpoint(address, token, health).await;
                        child = Some(process);
                    }
                    Err(error) => {
                        warn!(session_id, %error, "user workspace host did not become ready");
                        retry_after = tokio::time::Instant::now() + Duration::from_secs(2);
                    }
                }
            }
            Err(error) => {
                warn!(session_id, %error, "user workspace host could not be started");
                retry_after = tokio::time::Instant::now() + Duration::from_secs(2);
            }
        }
    }

    bridge.clear_endpoint().await;
    drop(child);
    Ok(())
}

async fn wait_for_workspace_host(
    client: &reqwest::Client,
    address: SocketAddr,
    token: &str,
    process: &WorkspaceHostProcess,
) -> Result<WorkspaceHealth> {
    let url = format!("http://{address}/health");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if process.has_exited() {
            return Err(anyhow!("workspace-host process exited during startup"));
        }
        if let Ok(response) = client.get(&url).bearer_auth(token).send().await
            && response.status().is_success()
        {
            return response
                .json()
                .await
                .context("workspace-host health response was invalid");
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out waiting for the workspace-host health endpoint"
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_session_host(
    client: &reqwest::Client,
    address: SocketAddr,
    token: &str,
    process: &SessionHostProcess,
) -> Result<()> {
    let url = format!("http://{address}/health");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if process.has_exited() {
            return Err(anyhow!("session-host process exited during startup"));
        }
        if let Ok(response) = client.get(&url).bearer_auth(token).send().await
            && response.status().is_success()
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out waiting for the session-host health endpoint"
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
