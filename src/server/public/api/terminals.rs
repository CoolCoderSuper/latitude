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
        response::ApiError,
        terminal_api::{
            PublicTerminalSessionListResponse, TerminalWsQuery, execute_terminal_command,
            parse_terminal_command_payload, root_terminal_info_response, terminal_info_response,
        },
    },
    state::AppState,
    terminal::TerminalSessionSummary,
    workspace::WorkspaceTerminalRequest,
};

use super::enabled_project;

pub(in crate::server) async fn public_api_get_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let project_config = enabled_project(&state, &project).await?;

    Ok(Json(terminal_info_response(
        &project,
        &project_config.project_dir,
    )))
}

pub(in crate::server) async fn public_api_post_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<impl IntoResponse, ApiError> {
    let project_config = enabled_project(&state, &project).await?;
    let command = terminal_command(req).await?;

    Ok(Json(
        execute_terminal_command(Some(&project_config.project_dir), command).await,
    ))
}

pub(in crate::server) async fn public_api_get_root_terminal() -> Response<Body> {
    Json(root_terminal_info_response().await).into_response()
}

pub(in crate::server) async fn public_api_post_root_terminal(
    req: Request<Body>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        execute_terminal_command(None, terminal_command(req).await?).await,
    ))
}

async fn terminal_command(req: Request<Body>) -> Result<String, ApiError> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = to_bytes(req.into_body(), MAX_TERMINAL_COMMAND_BYTES + 1024)
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("terminal payload could not be read: {error}"),
            )
        })?;
    parse_terminal_command_payload(content_type.as_deref(), &body)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error))
}

pub(in crate::server) async fn public_api_list_root_terminal_sessions(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    list_terminal_sessions(&state, TerminalTarget::root()).await
}

pub(in crate::server) async fn public_api_create_root_terminal_session(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    create_terminal_session(&state, TerminalTarget::root()).await
}

pub(in crate::server) async fn public_api_delete_root_terminal_session(
    AxumPath(session): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    delete_terminal_session(&state, TerminalTarget::root(), &session).await
}

pub(in crate::server) async fn public_api_list_terminal_sessions(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    list_terminal_sessions(&state, project_target(&state, project).await?).await
}

pub(in crate::server) async fn public_api_create_terminal_session(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    create_terminal_session(&state, project_target(&state, project).await?).await
}

pub(in crate::server) async fn public_api_delete_terminal_session(
    AxumPath((project, session)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    delete_terminal_session(&state, project_target(&state, project).await?, &session).await
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

async fn project_target(state: &AppState, project: String) -> Result<TerminalTarget, ApiError> {
    let config = enabled_project(state, &project).await?;
    Ok(TerminalTarget {
        project: Some(project),
        config: Some(config),
    })
}

async fn list_terminal_sessions(
    state: &AppState,
    target: TerminalTarget,
) -> Result<Json<PublicTerminalSessionListResponse>, ApiError> {
    let project = target.project.as_deref();
    let sessions = state.workspace().list_terminals(project).await?;
    Ok(Json(PublicTerminalSessionListResponse { sessions }))
}

async fn create_terminal_session(
    state: &AppState,
    target: TerminalTarget,
) -> Result<Json<TerminalSessionSummary>, ApiError> {
    let request = WorkspaceTerminalRequest {
        project: target.project,
        cwd: target.config.map(|config| config.project_dir),
        session: None,
    };
    Ok(Json(state.workspace().create_terminal(request).await?))
}

async fn delete_terminal_session(
    state: &AppState,
    target: TerminalTarget,
    session: &str,
) -> Result<StatusCode, ApiError> {
    let project = target.project.as_deref();
    let deleted = state.workspace().delete_terminal(project, session).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(terminal_not_found(session))
    }
}

fn terminal_not_found(session: &str) -> ApiError {
    ApiError::new(
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
            Err(error) => return error.into_response(),
        },
        None => TerminalTarget::root(),
    };

    let request = WorkspaceTerminalRequest {
        project: target.project,
        cwd: target.config.map(|config| config.project_dir),
        session: query.session.clone(),
    };
    let connection = match state.workspace().open_terminal(request).await {
        Ok(connection) => connection,
        Err(_) if query.session.is_some() => {
            return terminal_not_found(query.session.as_deref().unwrap_or_default())
                .into_response();
        }
        Err(error) => {
            return ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
        }
    };
    ws.on_upgrade(move |socket| connection.run(socket))
}
