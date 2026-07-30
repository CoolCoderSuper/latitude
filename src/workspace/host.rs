use std::{env, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result, anyhow};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, Response, StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post},
};
use tokio::net::TcpListener;
use tracing::info;

use crate::terminal::{TerminalSessionManager, root_terminal_cwd};

use super::{
    WorkspaceHealth, WorkspaceHostState,
    files::{
        WORKSPACE_FILE_WRITE_PATH, WORKSPACE_FILES_PATH, WorkspaceFiles, workspace_file_write,
        workspace_files,
    },
    process::{WORKSPACE_EXEC_PATH, workspace_exec},
    terminal::{
        WORKSPACE_TERMINAL_PATH, WORKSPACE_TERMINALS_PATH, workspace_create_terminal,
        workspace_delete_terminal, workspace_list_terminals, workspace_terminal,
    },
};

const WORKSPACE_HEALTH_PATH: &str = "/health";
const MAX_INTERNAL_REQUEST_BYTES: usize = 32 * 1024 * 1024;

pub(crate) async fn run_workspace_host(address: SocketAddr, token: String) -> Result<()> {
    if !address.ip().is_loopback() {
        return Err(anyhow!(
            "the workspace host must bind to a loopback address"
        ));
    }
    if token.len() < 32 {
        return Err(anyhow!(
            "the workspace-host token must contain at least 32 characters"
        ));
    }

    let state = WorkspaceHostState {
        token: Arc::from(token),
        terminals: Arc::new(TerminalSessionManager::default()),
        files: WorkspaceFiles::default(),
    };
    let router = Router::new()
        .route(WORKSPACE_HEALTH_PATH, get(workspace_health))
        .route(WORKSPACE_EXEC_PATH, post(workspace_exec))
        .route(
            WORKSPACE_TERMINALS_PATH,
            get(workspace_list_terminals).post(workspace_create_terminal),
        )
        .route(
            &format!("{WORKSPACE_TERMINALS_PATH}/{{session}}"),
            delete(workspace_delete_terminal),
        )
        .route(WORKSPACE_TERMINAL_PATH, get(workspace_terminal))
        .route(WORKSPACE_FILES_PATH, post(workspace_files))
        .route(WORKSPACE_FILE_WRITE_PATH, post(workspace_file_write))
        .layer(DefaultBodyLimit::max(MAX_INTERNAL_REQUEST_BYTES))
        .with_state(state);
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("workspace host could not bind {address}"))?;
    info!(
        bind = %listener.local_addr()?,
        identity = %workspace_identity(),
        "user workspace host listening"
    );
    axum::serve(listener, router).await?;
    Ok(())
}

async fn workspace_health(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(WorkspaceHealth {
        identity: workspace_identity(),
        profile_dir: root_terminal_cwd(),
    })
    .into_response()
}

fn workspace_identity() -> String {
    let user = env::var("USERNAME").unwrap_or_else(|_| "unknown".to_string());
    env::var("USERDOMAIN")
        .ok()
        .filter(|domain| !domain.is_empty())
        .map_or(user.clone(), |domain| format!("{domain}\\{user}"))
}

pub(super) fn workspace_is_authenticated(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|provided| provided == expected)
}

pub(super) fn workspace_error(status: StatusCode, message: impl Into<String>) -> Response<Body> {
    (
        status,
        Json(serde_json::json!({
            "error": message.into(),
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::workspace::{WorkspaceBridge, WorkspaceExecRequest};

    #[cfg(windows)]
    #[tokio::test]
    async fn workspace_bridge_executes_through_authenticated_loopback_host() {
        let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = probe.local_addr().unwrap();
        drop(probe);

        let token = "e".repeat(64);
        let host_token = token.clone();
        let host = tokio::spawn(async move {
            run_workspace_host(address, host_token).await.unwrap();
        });

        let client = reqwest::Client::new();
        let health_url = format!("http://{address}{WORKSPACE_HEALTH_PATH}");
        let health = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match client.get(&health_url).bearer_auth(&token).send().await {
                    Ok(response) if response.status().is_success() => {
                        break response.json::<WorkspaceHealth>().await.unwrap();
                    }
                    _ => tokio::time::sleep(Duration::from_millis(25)).await,
                }
            }
        })
        .await
        .expect("workspace host did not start");

        let unauthorized = client.get(&health_url).send().await.unwrap();
        assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);

        let bridge = WorkspaceBridge::new();
        bridge.set_endpoint(address, token, health).await;
        let output = bridge
            .execute(WorkspaceExecRequest::captured(
                "whoami.exe",
                Vec::new(),
                None,
                Duration::from_secs(10),
                4096,
            ))
            .await
            .unwrap();
        assert_eq!(output.status_code, Some(0));
        assert!(String::from_utf8_lossy(&output.stdout).contains('\\'));

        host.abort();
    }
}
