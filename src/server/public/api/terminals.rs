use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Request, Response, StatusCode, header},
    response::IntoResponse,
};

use crate::{
    config::ProjectConfig,
    server::{
        auth::{public_api_auth_challenge, public_headers_are_authenticated},
        constants::MAX_TERMINAL_COMMAND_BYTES,
        response::json_error,
        terminal_api::{
            PublicTerminalSessionListResponse, TerminalWsQuery, execute_terminal_command,
            parse_terminal_command_payload, root_terminal_info_response, terminal_info_response,
            terminal_websocket_session,
        },
    },
    state::AppState,
    workspace::WorkspaceTerminalRequest,
};

use super::enabled_project_or_response;

pub(in crate::server) async fn public_api_get_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };

    Json(terminal_info_response(
        &project,
        &project_config.project_dir,
    ))
    .into_response()
}

pub(in crate::server) async fn public_api_post_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };
    let command = match terminal_command(req).await {
        Ok(command) => command,
        Err(response) => return response,
    };

    Json(execute_terminal_command(Some(&project_config.project_dir), command).await).into_response()
}

pub(in crate::server) async fn public_api_get_root_terminal() -> Response<Body> {
    Json(root_terminal_info_response().await).into_response()
}

pub(in crate::server) async fn public_api_post_root_terminal(req: Request<Body>) -> Response<Body> {
    let command = match terminal_command(req).await {
        Ok(command) => command,
        Err(response) => return response,
    };
    Json(execute_terminal_command(None, command).await).into_response()
}

async fn terminal_command(req: Request<Body>) -> Result<String, Response<Body>> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = to_bytes(req.into_body(), MAX_TERMINAL_COMMAND_BYTES + 1024)
        .await
        .map_err(|error| {
            json_error(
                StatusCode::BAD_REQUEST,
                format!("terminal payload could not be read: {error}"),
            )
        })?;
    parse_terminal_command_payload(content_type.as_deref(), &body)
        .map_err(|error| json_error(StatusCode::BAD_REQUEST, error))
}

pub(in crate::server) async fn public_api_list_root_terminal_sessions(
    State(state): State<AppState>,
) -> Response<Body> {
    list_terminal_sessions(&state, TerminalTarget::root()).await
}

pub(in crate::server) async fn public_api_create_root_terminal_session(
    State(state): State<AppState>,
) -> Response<Body> {
    create_terminal_session(&state, TerminalTarget::root()).await
}

pub(in crate::server) async fn public_api_delete_root_terminal_session(
    AxumPath(session): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    delete_terminal_session(&state, TerminalTarget::root(), &session).await
}

pub(in crate::server) async fn public_api_list_terminal_sessions(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    let target = match project_target(&state, project).await {
        Ok(target) => target,
        Err(response) => return response,
    };
    list_terminal_sessions(&state, target).await
}

pub(in crate::server) async fn public_api_create_terminal_session(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    let target = match project_target(&state, project).await {
        Ok(target) => target,
        Err(response) => return response,
    };
    create_terminal_session(&state, target).await
}

pub(in crate::server) async fn public_api_delete_terminal_session(
    AxumPath((project, session)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Response<Body> {
    let target = match project_target(&state, project).await {
        Ok(target) => target,
        Err(response) => return response,
    };
    delete_terminal_session(&state, target, &session).await
}

struct TerminalTarget {
    project: Option<String>,
    config: Option<ProjectConfig>,
}

impl TerminalTarget {
    fn root() -> Self {
        Self {
            project: None,
            config: None,
        }
    }
}

async fn project_target(
    state: &AppState,
    project: String,
) -> Result<TerminalTarget, Response<Body>> {
    let config = enabled_project_or_response(state, &project).await?;
    Ok(TerminalTarget {
        project: Some(project),
        config: Some(config),
    })
}

async fn list_terminal_sessions(state: &AppState, target: TerminalTarget) -> Response<Body> {
    let project = target.project.as_deref();
    let sessions = if let Some(bridge) = state.workspace_bridge() {
        match bridge.list_terminals(project).await {
            Ok(sessions) => sessions,
            Err(error) => {
                return json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string());
            }
        }
    } else if let Some(project) = project {
        state.terminal_sessions().list_project(project).await
    } else {
        state.terminal_sessions().list_root().await
    };
    Json(PublicTerminalSessionListResponse { sessions }).into_response()
}

async fn create_terminal_session(state: &AppState, target: TerminalTarget) -> Response<Body> {
    if let Some(bridge) = state.workspace_bridge() {
        return match bridge
            .create_terminal(
                target.project,
                target.config.map(|config| config.project_dir),
            )
            .await
        {
            Ok(session) => Json(session).into_response(),
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }

    let sessions = state.terminal_sessions();
    let result = match (target.project, target.config) {
        (Some(project), Some(config)) => sessions
            .create_session(&project, &config.project_dir)
            .await
            .map(|session| session.summary()),
        _ => sessions
            .create_root_session()
            .await
            .map(|session| session.summary()),
    };
    match result {
        Ok(session) => Json(session).into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn delete_terminal_session(
    state: &AppState,
    target: TerminalTarget,
    session: &str,
) -> Response<Body> {
    let project = target.project.as_deref();
    let deleted = if let Some(bridge) = state.workspace_bridge() {
        match bridge.delete_terminal(project, session).await {
            Ok(deleted) => deleted,
            Err(error) => {
                return json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string());
            }
        }
    } else if let Some(project) = project {
        state
            .terminal_sessions()
            .close_project_session(project, session)
            .await
    } else {
        state.terminal_sessions().close_root_session(session).await
    };
    if deleted {
        StatusCode::NO_CONTENT.into_response()
    } else {
        terminal_not_found(session)
    }
}

fn terminal_not_found(session: &str) -> Response<Body> {
    json_error(
        StatusCode::NOT_FOUND,
        format!("terminal session '{session}' was not found"),
    )
}

pub(in crate::server) async fn public_terminal_ws(
    AxumPath(project): AxumPath<String>,
    Query(query): Query<TerminalWsQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    terminal_websocket(state, Some(project), query, headers, ws).await
}

pub(in crate::server) async fn public_root_terminal_ws(
    Query(query): Query<TerminalWsQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    terminal_websocket(state, None, query, headers, ws).await
}

async fn terminal_websocket(
    state: AppState,
    project: Option<String>,
    query: TerminalWsQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, query.token.as_deref()) {
        return public_api_auth_challenge();
    }

    let target = match project {
        Some(project) => match project_target(&state, project).await {
            Ok(target) => target,
            Err(response) => return response,
        },
        None => TerminalTarget::root(),
    };

    if let Some(bridge) = state.workspace_bridge() {
        let request = WorkspaceTerminalRequest {
            project: target.project,
            cwd: target.config.map(|config| config.project_dir),
            session: query.session,
        };
        return ws.on_upgrade(move |socket| async move {
            bridge.proxy_terminal(socket, request).await;
        });
    }

    let terminal_sessions = state.terminal_sessions();
    let session = match (target.project, target.config, query.session.as_deref()) {
        (Some(project), Some(_), Some(session)) => Ok(terminal_sessions
            .get_project_session(&project, session)
            .await),
        (None, None, Some(session)) => Ok(terminal_sessions.get_root_session(session).await),
        (Some(project), Some(config), None) => terminal_sessions
            .create_session(&project, &config.project_dir)
            .await
            .map(Some),
        (None, None, None) => terminal_sessions.create_root_session().await.map(Some),
        _ => Ok(None),
    };
    let session = match session {
        Ok(Some(session)) => session,
        Ok(None) => return terminal_not_found(query.session.as_deref().unwrap_or_default()),
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    };

    ws.on_upgrade(move |socket| terminal_websocket_session(socket, session))
}
