use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Method, Request, Response, StatusCode, header},
    response::IntoResponse,
};
use tracing::error;

use crate::{config::ProjectConfig, state::AppState};

use super::{
    super::{
        auth::{
            clean_next_path, parse_public_login_form, public_api_auth_challenge,
            public_auth_set_cookie, public_headers_are_authenticated, public_login_next_from_query,
            public_login_response, public_login_success_response, public_password_matches,
            public_request_is_authenticated,
        },
        constants::{
            AUTH_COOKIE_MAX_AGE_SECONDS, MAX_DIFF_ACTION_PAYLOAD_BYTES, MAX_LOGIN_PAYLOAD_BYTES,
            MAX_TERMINAL_COMMAND_BYTES, PUBLIC_API_PROJECTS_PATH,
        },
        git::{
            PublicGitActionResponse, collect_project_diff, execute_git_action,
            parse_public_git_action_payload, public_diff_response,
        },
        response::{ApiError, json_error, plain_response},
        terminal_api::{
            PublicTerminalSessionListResponse, TerminalWsQuery, execute_root_terminal_command,
            execute_terminal_command, parse_terminal_command_payload, root_terminal_info_response,
            terminal_info_response, terminal_websocket_session,
        },
    },
    models::{
        PublicLoginPayload, PublicLoginResponse, PublicProjectListResponse, PublicSessionResponse,
        public_project_detail, public_project_summary, public_root_desktop_link,
        public_root_terminal_link,
    },
};

pub(in crate::server) async fn get_public_login(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let next = clean_next_path(public_login_next_from_query(req.uri().query()));
    public_login_response(
        StatusCode::OK,
        &next,
        false,
        req.method() == Method::HEAD,
        state.device_hostname(),
    )
}

pub(in crate::server) async fn post_public_login(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    let query_next = public_login_next_from_query(req.uri().query());
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_LOGIN_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return plain_response(
                StatusCode::BAD_REQUEST,
                format!("login payload could not be read: {error}\n"),
            );
        }
    };
    let form = parse_public_login_form(&body);
    let next = clean_next_path(form.next.or(query_next));

    if public_password_matches(&form.password, &config.public_password) {
        return public_login_success_response(
            &next,
            public_auth_set_cookie(&state, &config.public_password),
        );
    }

    public_login_response(
        StatusCode::UNAUTHORIZED,
        &next,
        true,
        false,
        state.device_hostname(),
    )
}

pub(in crate::server) async fn public_api_session(
    State(state): State<AppState>,
    req: Request<Body>,
) -> impl IntoResponse {
    let config = state.config_snapshot().await;
    let authenticated = public_request_is_authenticated(&state, &config, &req);

    Json(PublicSessionResponse {
        authenticated,
        projects_href: authenticated.then(|| PUBLIC_API_PROJECTS_PATH.to_string()),
        root_terminal: authenticated.then(public_root_terminal_link),
        root_desktop: authenticated
            .then(|| public_root_desktop_link(&config.desktop))
            .flatten(),
        device_hostname: state.device_hostname().to_string(),
    })
}

pub(in crate::server) async fn public_api_login(
    State(state): State<AppState>,
    Json(payload): Json<PublicLoginPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    if !public_password_matches(&payload.password, &config.public_password) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "incorrect password",
        ));
    }

    let token = state.public_auth_cookie_value(&config.public_password);
    Ok((
        StatusCode::OK,
        [
            (
                header::SET_COOKIE,
                public_auth_set_cookie(&state, &config.public_password),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(PublicLoginResponse {
            token,
            max_age_seconds: AUTH_COOKIE_MAX_AGE_SECONDS,
            projects_href: PUBLIC_API_PROJECTS_PATH.to_string(),
            root_terminal: public_root_terminal_link(),
            root_desktop: public_root_desktop_link(&config.desktop),
            device_hostname: state.device_hostname().to_string(),
        }),
    ))
}

pub(in crate::server) async fn public_api_list_projects(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let catalog_projects = match list_catalog_projects_or_response(&state).await {
        Ok(projects) => projects,
        Err(response) => return response,
    };
    let dirty_projects = super::dirty_project_names(&catalog_projects).await;
    let projects = catalog_projects
        .iter()
        .filter(|project| project.enabled)
        .map(|project| public_project_summary(project, dirty_projects.contains(&project.name)))
        .collect();

    Json(PublicProjectListResponse {
        device_hostname: state.device_hostname().to_string(),
        root_terminal: public_root_terminal_link(),
        root_desktop: public_root_desktop_link(&config.desktop),
        projects,
    })
    .into_response()
}

pub(in crate::server) async fn public_api_get_project(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    };

    Json(public_project_detail(
        &project_config,
        state.device_hostname(),
    ))
    .into_response()
}

pub(in crate::server) async fn public_api_get_project_diff(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    };

    Json(public_diff_response(
        collect_project_diff(&project_config.project_dir).await,
    ))
    .into_response()
}

pub(in crate::server) async fn public_api_patch_project_diff(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    };

    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_DIFF_ACTION_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("action payload could not be read: {error}"),
            );
        }
    };
    let action = match parse_public_git_action_payload(content_type.as_deref(), &body) {
        Ok(action) => action,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
    };

    let action_result = execute_git_action(&project_config.project_dir, action).await;
    if let Err(error) = &action_result {
        error!(%error, project = %project_config.name, "git action failed");
    }

    Json(PublicGitActionResponse {
        ok: action_result.is_ok(),
        error: action_result.err(),
        diff: public_diff_response(collect_project_diff(&project_config.project_dir).await),
    })
    .into_response()
}

pub(in crate::server) async fn public_api_get_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    };

    Json(terminal_info_response(&project, &project_config.project_dir).await).into_response()
}

pub(in crate::server) async fn public_api_post_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    };

    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_TERMINAL_COMMAND_BYTES + 1024).await {
        Ok(body) => body,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("terminal payload could not be read: {error}"),
            );
        }
    };
    let command = match parse_terminal_command_payload(content_type.as_deref(), &body) {
        Ok(command) => command,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
    };

    Json(execute_terminal_command(&project_config.project_dir, command).await).into_response()
}

pub(in crate::server) async fn public_api_get_root_terminal(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    Json(root_terminal_info_response().await).into_response()
}

pub(in crate::server) async fn public_api_post_root_terminal(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_TERMINAL_COMMAND_BYTES + 1024).await {
        Ok(body) => body,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("terminal payload could not be read: {error}"),
            );
        }
    };
    let command = match parse_terminal_command_payload(content_type.as_deref(), &body) {
        Ok(command) => command,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
    };

    Json(execute_root_terminal_command(command).await).into_response()
}

pub(in crate::server) async fn public_api_list_root_terminal_sessions(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    Json(PublicTerminalSessionListResponse {
        sessions: state.terminal_sessions().list_root().await,
    })
    .into_response()
}

pub(in crate::server) async fn public_api_create_root_terminal_session(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    match state.terminal_sessions().create_root_session().await {
        Ok(session) => Json(session.summary()).into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

pub(in crate::server) async fn public_api_delete_root_terminal_session(
    AxumPath(session): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    if state.terminal_sessions().close_root_session(&session).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        json_error(
            StatusCode::NOT_FOUND,
            format!("terminal session '{session}' was not found"),
        )
    }
}

pub(in crate::server) async fn public_api_list_terminal_sessions(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    match enabled_project_or_response(&state, &project).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    }

    Json(PublicTerminalSessionListResponse {
        sessions: state.terminal_sessions().list_project(&project).await,
    })
    .into_response()
}

pub(in crate::server) async fn public_api_create_terminal_session(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    };

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
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    match enabled_project_or_response(&state, &project).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    }

    if state
        .terminal_sessions()
        .close_project_session(&project, &session)
        .await
    {
        StatusCode::NO_CONTENT.into_response()
    } else {
        json_error(
            StatusCode::NOT_FOUND,
            format!("terminal session '{session}' was not found"),
        )
    }
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
        Ok(Some(project)) => project,
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("project '{project}' was not found"),
            );
        }
        Err(response) => return response,
    };

    let terminal_sessions = state.terminal_sessions();
    let session = if let Some(session_id) = query.session.as_deref() {
        match terminal_sessions
            .get_project_session(&project, session_id)
            .await
        {
            Some(session) => session,
            None => {
                return json_error(
                    StatusCode::NOT_FOUND,
                    format!("terminal session '{session_id}' was not found"),
                );
            }
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

async fn list_catalog_projects_or_response(
    state: &AppState,
) -> Result<Vec<ProjectConfig>, Response<Body>> {
    state.catalog().list_projects().await.map_err(|error| {
        error!(%error, "project list failed");
        json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "catalog could not be read",
        )
    })
}

async fn enabled_project_or_response(
    state: &AppState,
    project: &str,
) -> Result<Option<ProjectConfig>, Response<Body>> {
    state
        .catalog()
        .get_project(project)
        .await
        .map(|project| project.filter(|project| project.enabled))
        .map_err(|error| {
            error!(%error, project, "project lookup failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog could not be read",
            )
        })
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

    let terminal_sessions = state.terminal_sessions();
    let session = if let Some(session_id) = query.session.as_deref() {
        match terminal_sessions.get_root_session(session_id).await {
            Some(session) => session,
            None => {
                return json_error(
                    StatusCode::NOT_FOUND,
                    format!("terminal session '{session_id}' was not found"),
                );
            }
        }
    } else {
        match terminal_sessions.create_root_session().await {
            Ok(session) => session,
            Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
        }
    };

    ws.on_upgrade(move |socket| terminal_websocket_session(socket, session))
}
