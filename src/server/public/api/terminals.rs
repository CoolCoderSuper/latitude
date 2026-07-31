use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Request, Response, StatusCode, header},
    response::IntoResponse,
};

use crate::{
    server::{
        auth::{public_api_auth_challenge, public_headers_are_authenticated},
        constants::MAX_TERMINAL_COMMAND_BYTES,
        response::json_error,
        terminal_api::{
            PublicTerminalSessionListResponse, TerminalWsQuery, execute_root_terminal_command,
            execute_terminal_command, parse_terminal_command_payload, root_terminal_info_response,
            terminal_info_response, terminal_websocket_session,
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

    Json(execute_terminal_command(&project_config.project_dir, command).await).into_response()
}

pub(in crate::server) async fn public_api_get_root_terminal() -> Response<Body> {
    Json(root_terminal_info_response().await).into_response()
}

pub(in crate::server) async fn public_api_post_root_terminal(req: Request<Body>) -> Response<Body> {
    let command = match terminal_command(req).await {
        Ok(command) => command,
        Err(response) => return response,
    };
    Json(execute_root_terminal_command(command).await).into_response()
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
    if let Some(bridge) = state.workspace_bridge() {
        return match bridge.list_terminals(None).await {
            Ok(sessions) => Json(PublicTerminalSessionListResponse { sessions }).into_response(),
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }

    Json(PublicTerminalSessionListResponse {
        sessions: state.terminal_sessions().list_root().await,
    })
    .into_response()
}

pub(in crate::server) async fn public_api_create_root_terminal_session(
    State(state): State<AppState>,
) -> Response<Body> {
    if let Some(bridge) = state.workspace_bridge() {
        return match bridge.create_terminal(None, None).await {
            Ok(session) => Json(session).into_response(),
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }

    match state.terminal_sessions().create_root_session().await {
        Ok(session) => Json(session.summary()).into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

pub(in crate::server) async fn public_api_delete_root_terminal_session(
    AxumPath(session): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    if let Some(bridge) = state.workspace_bridge() {
        return match bridge.delete_terminal(None, &session).await {
            Ok(true) => StatusCode::NO_CONTENT.into_response(),
            Ok(false) => terminal_not_found(&session),
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }

    if state.terminal_sessions().close_root_session(&session).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        terminal_not_found(&session)
    }
}

pub(in crate::server) async fn public_api_list_terminal_sessions(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    if let Err(response) = enabled_project_or_response(&state, &project).await {
        return response;
    }

    if let Some(bridge) = state.workspace_bridge() {
        return match bridge.list_terminals(Some(&project)).await {
            Ok(sessions) => Json(PublicTerminalSessionListResponse { sessions }).into_response(),
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }

    Json(PublicTerminalSessionListResponse {
        sessions: state.terminal_sessions().list_project(&project).await,
    })
    .into_response()
}

pub(in crate::server) async fn public_api_create_terminal_session(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };

    if let Some(bridge) = state.workspace_bridge() {
        return match bridge
            .create_terminal(Some(project), Some(project_config.project_dir.clone()))
            .await
        {
            Ok(session) => Json(session).into_response(),
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }

    match state
        .terminal_sessions()
        .create_session(&project, &project_config.project_dir)
        .await
    {
        Ok(session) => Json(session.summary()).into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

pub(in crate::server) async fn public_api_delete_terminal_session(
    AxumPath((project, session)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Response<Body> {
    if let Err(response) = enabled_project_or_response(&state, &project).await {
        return response;
    }

    if let Some(bridge) = state.workspace_bridge() {
        return match bridge.delete_terminal(Some(&project), &session).await {
            Ok(true) => StatusCode::NO_CONTENT.into_response(),
            Ok(false) => terminal_not_found(&session),
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }

    if state
        .terminal_sessions()
        .close_project_session(&project, &session)
        .await
    {
        StatusCode::NO_CONTENT.into_response()
    } else {
        terminal_not_found(&session)
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
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, query.token.as_deref()) {
        return public_api_auth_challenge();
    }

    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };

    if let Some(bridge) = state.workspace_bridge() {
        let request = WorkspaceTerminalRequest {
            project: Some(project),
            cwd: Some(project_config.project_dir),
            session: query.session,
        };
        return ws.on_upgrade(move |socket| async move {
            bridge.proxy_terminal(socket, request).await;
        });
    }

    let terminal_sessions = state.terminal_sessions();
    let session = if let Some(session_id) = query.session.as_deref() {
        match terminal_sessions
            .get_project_session(&project, session_id)
            .await
        {
            Some(session) => session,
            None => return terminal_not_found(session_id),
        }
    } else {
        match terminal_sessions
            .create_session(&project, &project_config.project_dir)
            .await
        {
            Ok(session) => session,
            Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
        }
    };

    ws.on_upgrade(move |socket| terminal_websocket_session(socket, session))
}

pub(in crate::server) async fn public_root_terminal_ws(
    Query(query): Query<TerminalWsQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, query.token.as_deref()) {
        return public_api_auth_challenge();
    }

    if let Some(bridge) = state.workspace_bridge() {
        let request = WorkspaceTerminalRequest {
            project: None,
            cwd: None,
            session: query.session,
        };
        return ws.on_upgrade(move |socket| async move {
            bridge.proxy_terminal(socket, request).await;
        });
    }

    let terminal_sessions = state.terminal_sessions();
    let session = if let Some(session_id) = query.session.as_deref() {
        match terminal_sessions.get_root_session(session_id).await {
            Some(session) => session,
            None => return terminal_not_found(session_id),
        }
    } else {
        match terminal_sessions.create_root_session().await {
            Ok(session) => session,
            Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
        }
    };

    ws.on_upgrade(move |socket| terminal_websocket_session(socket, session))
}
