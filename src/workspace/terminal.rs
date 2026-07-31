use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Json,
    body::Body,
    extract::{
        Path as AxumPath, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Response, StatusCode, header},
    response::IntoResponse,
};
use futures_util::SinkExt;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest},
};
use tracing::{debug, warn};

use crate::{
    server::terminal_websocket_session,
    terminal::{TerminalSession, TerminalSessionManager, TerminalSessionSummary},
    websocket_bridge::forward_websocket,
};

use super::{
    WorkspaceBridge, WorkspaceHostState, WorkspaceServices, WorkspaceTerminalRequest,
    bridge::workspace_success_response,
    host::{workspace_error, workspace_is_authenticated},
};

pub(super) const WORKSPACE_TERMINALS_PATH: &str = "/terminals";
pub(super) const WORKSPACE_TERMINAL_PATH: &str = "/terminal";

pub(crate) enum WorkspaceTerminalConnection {
    Local(Arc<TerminalSession>),
    Remote(WorkspaceBridge, WorkspaceTerminalRequest),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct WorkspaceTerminalQuery {
    project: Option<String>,
}

impl WorkspaceServices {
    pub(crate) async fn list_terminals(
        &self,
        project: Option<&str>,
    ) -> Result<Vec<TerminalSessionSummary>, (StatusCode, String)> {
        match &self.bridge {
            Some(bridge) => bridge
                .list_terminals(project)
                .await
                .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error.to_string())),
            None => Ok(match project {
                Some(project) => self.terminals.list_project(project).await,
                None => self.terminals.list_root().await,
            }),
        }
    }

    pub(crate) async fn create_terminal(
        &self,
        request: WorkspaceTerminalRequest,
    ) -> Result<TerminalSessionSummary, (StatusCode, String)> {
        match &self.bridge {
            Some(bridge) => bridge
                .create_terminal(request.project, request.cwd)
                .await
                .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error.to_string())),
            None => create_workspace_terminal(&self.terminals, &request)
                .await
                .map(|session| session.summary())
                .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error)),
        }
    }

    pub(crate) async fn delete_terminal(
        &self,
        project: Option<&str>,
        session: &str,
    ) -> Result<bool, (StatusCode, String)> {
        match &self.bridge {
            Some(bridge) => bridge
                .delete_terminal(project, session)
                .await
                .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error.to_string())),
            None => Ok(match project {
                Some(project) => self.terminals.close_project_session(project, session).await,
                None => self.terminals.close_root_session(session).await,
            }),
        }
    }

    pub(crate) async fn open_terminal(
        &self,
        request: WorkspaceTerminalRequest,
    ) -> Result<WorkspaceTerminalConnection, String> {
        if let Some(bridge) = &self.bridge {
            return Ok(WorkspaceTerminalConnection::Remote(bridge.clone(), request));
        }
        let session = match request.session.as_deref() {
            Some(session) => match request.project.as_deref() {
                Some(project) => self.terminals.get_project_session(project, session).await,
                None => self.terminals.get_root_session(session).await,
            }
            .ok_or_else(|| format!("terminal session '{session}' was not found"))?,
            None => create_workspace_terminal(&self.terminals, &request).await?,
        };
        Ok(WorkspaceTerminalConnection::Local(session))
    }
}

impl WorkspaceTerminalConnection {
    pub(crate) async fn run(self, socket: WebSocket) {
        match self {
            Self::Local(session) => terminal_websocket_session(socket, session).await,
            Self::Remote(bridge, request) => bridge.proxy_terminal(socket, request).await,
        }
    }
}

impl WorkspaceBridge {
    pub(crate) async fn list_terminals(
        &self,
        project: Option<&str>,
    ) -> Result<Vec<TerminalSessionSummary>> {
        let endpoint = self.endpoint().await?;
        let url = format!("http://{}{}", endpoint.address, WORKSPACE_TERMINALS_PATH);
        let response = self
            .client
            .get(url)
            .bearer_auth(&endpoint.token)
            .query(&WorkspaceTerminalQuery {
                project: project.map(str::to_string),
            })
            .send()
            .await
            .context("workspace terminal host is unavailable")?;
        workspace_success_response(response)
            .await?
            .json()
            .await
            .context("workspace terminal list was invalid")
    }

    pub(crate) async fn create_terminal(
        &self,
        project: Option<String>,
        cwd: Option<PathBuf>,
    ) -> Result<TerminalSessionSummary> {
        let endpoint = self.endpoint().await?;
        let url = format!("http://{}{}", endpoint.address, WORKSPACE_TERMINALS_PATH);
        let response = self
            .client
            .post(url)
            .bearer_auth(&endpoint.token)
            .json(&WorkspaceTerminalRequest {
                project,
                cwd,
                session: None,
            })
            .send()
            .await
            .context("workspace terminal host is unavailable")?;
        workspace_success_response(response)
            .await?
            .json()
            .await
            .context("workspace terminal response was invalid")
    }

    pub(crate) async fn delete_terminal(
        &self,
        project: Option<&str>,
        session: &str,
    ) -> Result<bool> {
        let endpoint = self.endpoint().await?;
        let url = format!(
            "http://{}{}/{}",
            endpoint.address, WORKSPACE_TERMINALS_PATH, session
        );
        let response = self
            .client
            .request(Method::DELETE, url)
            .bearer_auth(&endpoint.token)
            .query(&WorkspaceTerminalQuery {
                project: project.map(str::to_string),
            })
            .send()
            .await
            .context("workspace terminal host is unavailable")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        workspace_success_response(response).await?;
        Ok(true)
    }

    pub(crate) async fn proxy_terminal(
        &self,
        mut browser: WebSocket,
        request: WorkspaceTerminalRequest,
    ) {
        if let Err(error) = self.run_terminal_proxy(&mut browser, request).await {
            warn!(%error, "workspace terminal proxy failed");
            let _ = browser
                .send(Message::Text(
                    format!("\r\n[Latitude workspace error: {error}]\r\n").into(),
                ))
                .await;
        }
    }

    async fn run_terminal_proxy(
        &self,
        browser: &mut WebSocket,
        terminal_request: WorkspaceTerminalRequest,
    ) -> Result<()> {
        let endpoint = self.endpoint().await?;
        let url = format!("ws://{}{}", endpoint.address, WORKSPACE_TERMINAL_PATH);
        let mut request = url
            .into_client_request()
            .context("workspace terminal WebSocket URL was invalid")?;
        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {}", endpoint.token)
                .parse()
                .context("workspace authorization header was invalid")?,
        );
        let (mut worker, _) = connect_async(request)
            .await
            .context("workspace terminal host is unavailable")?;
        worker
            .send(tungstenite::Message::Text(
                serde_json::to_string(&terminal_request)?.into(),
            ))
            .await
            .context("workspace terminal parameters could not be sent")?;

        forward_websocket(browser, &mut worker).await?;
        debug!("workspace terminal proxy closed");
        Ok(())
    }
}

pub(super) async fn workspace_list_terminals(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    Query(query): Query<WorkspaceTerminalQuery>,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let sessions = if let Some(project) = query.project {
        state.terminals.list_project(&project).await
    } else {
        state.terminals.list_root().await
    };
    Json(sessions).into_response()
}

pub(super) async fn workspace_create_terminal(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    Json(request): Json<WorkspaceTerminalRequest>,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match create_workspace_terminal(&state.terminals, &request).await {
        Ok(session) => Json(session.summary()).into_response(),
        Err(error) => workspace_error(StatusCode::BAD_REQUEST, error),
    }
}

pub(super) async fn workspace_delete_terminal(
    AxumPath(session): AxumPath<String>,
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    Query(query): Query<WorkspaceTerminalQuery>,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let removed = if let Some(project) = query.project {
        state
            .terminals
            .close_project_session(&project, &session)
            .await
    } else {
        state.terminals.close_root_session(&session).await
    };
    if removed {
        StatusCode::NO_CONTENT.into_response()
    } else {
        workspace_error(StatusCode::NOT_FOUND, "terminal session was not found")
    }
}

pub(super) async fn workspace_terminal(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(move |mut socket| async move {
        let request = match socket.recv().await {
            Some(Ok(Message::Text(message))) => {
                serde_json::from_slice::<WorkspaceTerminalRequest>(message.as_bytes())
            }
            _ => return,
        };
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                let _ = socket
                    .send(Message::Text(
                        format!("\r\n[Invalid workspace terminal request: {error}]\r\n").into(),
                    ))
                    .await;
                return;
            }
        };
        let session = if let Some(session_id) = request.session.as_deref() {
            if let Some(project) = request.project.as_deref() {
                state
                    .terminals
                    .get_project_session(project, session_id)
                    .await
            } else {
                state.terminals.get_root_session(session_id).await
            }
            .ok_or_else(|| format!("terminal session '{session_id}' was not found"))
        } else {
            create_workspace_terminal(&state.terminals, &request).await
        };
        match session {
            Ok(session) => terminal_websocket_session(socket, session).await,
            Err(error) => {
                let _ = socket
                    .send(Message::Text(
                        format!("\r\n[Latitude workspace error: {error}]\r\n").into(),
                    ))
                    .await;
            }
        }
    })
}

async fn create_workspace_terminal(
    terminals: &TerminalSessionManager,
    request: &WorkspaceTerminalRequest,
) -> Result<Arc<TerminalSession>, String> {
    if let Some(project) = request.project.as_deref() {
        let cwd = request
            .cwd
            .as_deref()
            .ok_or_else(|| "project terminal directory is required".to_string())?;
        terminals.create_session(project, cwd).await
    } else {
        terminals.create_root_session().await
    }
}
