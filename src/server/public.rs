use std::{path::Path, time::Duration};

use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Method, Request, Response, StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::error;

use crate::{
    config::{ApplicationConfig, ApplicationTarget, LatitudeConfig, PageFormat, ProjectConfig},
    state::AppState,
};

use super::{
    auth::{
        clean_next_path, parse_public_login_form, public_api_auth_challenge, public_auth_challenge,
        public_auth_set_cookie, public_headers_are_authenticated, public_login_next_from_query,
        public_login_response, public_login_success_response, public_password_matches,
        public_request_is_authenticated, request_bearer_token,
    },
    constants::{
        AUTH_COOKIE_MAX_AGE_SECONDS, AUTH_COOKIE_NAME, DIFF_ROUTE_SEGMENT,
        MAX_DIFF_ACTION_PAYLOAD_BYTES, MAX_LOGIN_PAYLOAD_BYTES, MAX_TERMINAL_COMMAND_BYTES,
        PUBLIC_API_PROJECTS_PATH, TERMINAL_ROUTE_SEGMENT,
    },
    git::{
        GitActionResponse, PublicGitActionResponse, collect_project_diff, execute_git_action,
        handle_git_action_request, parse_public_git_action_payload, public_diff_response,
    },
    page::{page_theme_from_headers, render_page_content},
    paths::{
        ProjectPath, filtered_cookie_header, is_hop_by_hop_header, join_upstream_url,
        resolve_project_path, sanitized_relative_path, split_project_path,
    },
    render::{
        deployment_home_label, deployment_kind, deployment_page_title, enabled_deployment_count,
        project_summary, render_diff_workspace_fragment, render_project_diff, render_project_home,
        render_project_terminal, render_server_home,
    },
    response::{ApiError, internal_response, json_error, plain_response},
    terminal_api::{
        PublicTerminalSessionListResponse, TerminalWsQuery, execute_terminal_command,
        parse_terminal_command_payload, terminal_info_response, terminal_websocket_session,
    },
};

#[derive(Debug, Deserialize)]
pub(super) struct PublicLoginPayload {
    pub(super) password: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicSessionResponse {
    pub(super) authenticated: bool,
    pub(super) projects_href: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicLoginResponse {
    pub(super) token: String,
    pub(super) max_age_seconds: u64,
    pub(super) projects_href: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicProjectListResponse {
    pub(super) projects: Vec<PublicProjectSummary>,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicProjectSummary {
    pub(super) name: String,
    pub(super) href: String,
    pub(super) api_href: String,
    pub(super) summary: String,
    pub(super) deployment_count: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicProjectDetail {
    pub(super) name: String,
    pub(super) href: String,
    pub(super) api_href: String,
    pub(super) summary: String,
    pub(super) deployment_count: usize,
    pub(super) diff: PublicProjectDiffLink,
    pub(super) terminal: PublicProjectTerminalLink,
    pub(super) deployments: Vec<PublicDeploymentSummary>,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicProjectDiffLink {
    pub(super) href: String,
    pub(super) api_href: String,
    pub(super) label: &'static str,
    pub(super) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicProjectTerminalLink {
    pub(super) href: String,
    pub(super) api_href: String,
    pub(super) label: &'static str,
    pub(super) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicDeploymentSummary {
    pub(super) name: String,
    pub(super) href: String,
    pub(super) kind: &'static str,
    pub(super) label: &'static str,
    pub(super) title: Option<String>,
}

pub(super) fn public_project_summary(project: &ProjectConfig) -> PublicProjectSummary {
    let deployment_count = enabled_deployment_count(project);
    PublicProjectSummary {
        name: project.name.clone(),
        href: format!("/{}", project.name),
        api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}", project.name),
        summary: project_summary(project),
        deployment_count,
    }
}

pub(super) fn public_project_detail(project: &ProjectConfig) -> PublicProjectDetail {
    let deployments = project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .map(public_deployment_summary(project))
        .collect::<Vec<_>>();

    PublicProjectDetail {
        name: project.name.clone(),
        href: format!("/{}", project.name),
        api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}", project.name),
        summary: project_summary(project),
        deployment_count: deployments.len(),
        diff: PublicProjectDiffLink {
            href: format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT),
            api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}/diff", project.name),
            label: "Code changes",
            description: "Review staged and unstaged files",
        },
        terminal: PublicProjectTerminalLink {
            href: format!("/{}/{}", project.name, TERMINAL_ROUTE_SEGMENT),
            api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}/terminal", project.name),
            label: "Terminal",
            description: "Run commands in the project directory",
        },
        deployments,
    }
}

fn public_deployment_summary(
    project: &ProjectConfig,
) -> impl Fn(&ApplicationConfig) -> PublicDeploymentSummary + '_ {
    |deployment| PublicDeploymentSummary {
        name: deployment.name.clone(),
        href: format!("/{}/{}", project.name, deployment.name),
        kind: deployment_kind(deployment),
        label: deployment_home_label(deployment),
        title: deployment_page_title(deployment).map(str::to_string),
    }
}

pub(super) async fn public_entry(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let original_path = req.uri().path().to_string();
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_auth_challenge(&req, false);
    }

    if original_path == "/" {
        return serve_server_home(req, &config).await;
    }

    let Some(public_path) = split_project_path(&original_path) else {
        return plain_response(
            StatusCode::NOT_FOUND,
            "Latitude is running. Mount a deployment at /{project}/{name} to serve traffic.\n",
        );
    };
    let project_mount = public_path.project_name().to_string();

    let Some(project) = config
        .projects
        .iter()
        .find(|project| project.enabled && project.name == project_mount)
        .cloned()
    else {
        return plain_response(
            StatusCode::NOT_FOUND,
            format!("No enabled project is mounted at /{project_mount}\n"),
        );
    };

    let ProjectPath::Deployment {
        deployment: app_mount,
        remainder,
        ..
    } = public_path
    else {
        return serve_project_home(req, &project).await;
    };

    if app_mount == DIFF_ROUTE_SEGMENT {
        return serve_project_diff(req, &project, remainder.as_str()).await;
    }
    if app_mount == TERMINAL_ROUTE_SEGMENT {
        return serve_project_terminal(req, &project, remainder.as_str()).await;
    }

    let Some(app) = project
        .deployments
        .iter()
        .find(|app| app.enabled && app.name == app_mount)
        .cloned()
    else {
        return plain_response(
            StatusCode::NOT_FOUND,
            format!("No enabled deployment is mounted at /{project_mount}/{app_mount}\n"),
        );
    };

    match &app.target {
        ApplicationTarget::ReverseProxy {
            upstream,
            strip_prefix,
        } => proxy_request(state, req, upstream, *strip_prefix, remainder.as_str()).await,
        ApplicationTarget::Static {
            root,
            index_file,
            spa_fallback,
        } => {
            let root = resolve_project_path(&project.project_dir, root);
            serve_static(req, &root, index_file, *spa_fallback, remainder.as_str()).await
        }
        ApplicationTarget::Page {
            content,
            format,
            title,
        } => serve_page(req, title.as_deref(), *format, content, remainder.as_str()).await,
    }
}

pub(super) async fn get_public_login(req: Request<Body>) -> Response<Body> {
    let next = clean_next_path(public_login_next_from_query(req.uri().query()));
    public_login_response(StatusCode::OK, &next, false, req.method() == Method::HEAD)
}

pub(super) async fn post_public_login(
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

    public_login_response(StatusCode::UNAUTHORIZED, &next, true, false)
}

pub(super) async fn public_api_session(
    State(state): State<AppState>,
    req: Request<Body>,
) -> impl IntoResponse {
    let config = state.config_snapshot().await;
    let authenticated = public_request_is_authenticated(&state, &config, &req);

    Json(PublicSessionResponse {
        authenticated,
        projects_href: authenticated.then(|| PUBLIC_API_PROJECTS_PATH.to_string()),
    })
}

pub(super) async fn public_api_login(
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
        }),
    ))
}

pub(super) async fn public_api_list_projects(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let projects = config
        .projects
        .iter()
        .filter(|project| project.enabled)
        .map(public_project_summary)
        .collect();

    Json(PublicProjectListResponse { projects }).into_response()
}

pub(super) async fn public_api_get_project(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    Json(public_project_detail(project)).into_response()
}

pub(super) async fn public_api_get_project_diff(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    Json(public_diff_response(
        collect_project_diff(&project.project_dir).await,
    ))
    .into_response()
}

pub(super) async fn public_api_patch_project_diff(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
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

pub(super) async fn public_api_get_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    Json(terminal_info_response(&project, &project_config.project_dir).await).into_response()
}

pub(super) async fn public_api_post_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
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

pub(super) async fn public_api_list_terminal_sessions(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    if !config
        .projects
        .iter()
        .any(|item| item.enabled && item.name == project)
    {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    }

    Json(PublicTerminalSessionListResponse {
        sessions: state.terminal_sessions().list_project(&project).await,
    })
    .into_response()
}

pub(super) async fn public_api_create_terminal_session(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
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

pub(super) async fn public_api_delete_terminal_session(
    AxumPath((project, session)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    if !config
        .projects
        .iter()
        .any(|item| item.enabled && item.name == project)
    {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
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

pub(super) async fn public_terminal_ws(
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

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
        .cloned()
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
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

pub(super) async fn proxy_request(
    state: AppState,
    req: Request<Body>,
    upstream: &str,
    strip_prefix: bool,
    remainder: &str,
) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let forward_path = if strip_prefix {
        remainder.to_string()
    } else {
        parts.uri.path().to_string()
    };

    let target_url = match join_upstream_url(upstream, &forward_path, parts.uri.query()) {
        Ok(url) => url,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("upstream URL could not be built: {error}"),
            );
        }
    };

    let body_bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("request body could not be read: {error}"),
            );
        }
    };

    let mut builder = state.client().request(parts.method, target_url);
    for (name, value) in &parts.headers {
        if is_hop_by_hop_header(name.as_str()) || *name == header::HOST {
            continue;
        }
        if *name == header::COOKIE {
            if let Some(filtered_cookie) = filtered_cookie_header(value, AUTH_COOKIE_NAME) {
                builder = builder.header(name, filtered_cookie);
            }
            continue;
        }
        builder = builder.header(name, value);
    }

    match builder
        .timeout(Duration::from_secs(60))
        .body(body_bytes)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let mut response_builder = Response::builder().status(status);

            for (name, value) in response.headers() {
                if is_hop_by_hop_header(name.as_str()) {
                    continue;
                }
                response_builder = response_builder.header(name, value);
            }

            match response.bytes().await {
                Ok(bytes) => response_builder
                    .body(Body::from(bytes))
                    .unwrap_or_else(internal_response),
                Err(error) => json_error(
                    StatusCode::BAD_GATEWAY,
                    format!("upstream body could not be read: {error}"),
                ),
            }
        }
        Err(error) => json_error(
            StatusCode::BAD_GATEWAY,
            format!("upstream request failed: {error}"),
        ),
    }
}

pub(super) async fn serve_static(
    req: Request<Body>,
    root: &Path,
    index_file: &str,
    spa_fallback: bool,
    remainder: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "static deployments support GET and HEAD\n",
        );
    }

    let relative_path = match sanitized_relative_path(remainder) {
        Some(path) => path,
        None => return plain_response(StatusCode::BAD_REQUEST, "invalid static path\n"),
    };

    let mut candidate = root.join(relative_path);
    match fs::metadata(&candidate).await {
        Ok(metadata) if metadata.is_dir() => {
            candidate = candidate.join(index_file);
        }
        Ok(_) => {}
        Err(_) if spa_fallback => {
            candidate = root.join(index_file);
        }
        Err(_) => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
    }

    let metadata = match fs::metadata(&candidate).await {
        Ok(metadata) if metadata.is_file() => metadata,
        _ if spa_fallback => match fs::metadata(root.join(index_file)).await {
            Ok(metadata) if metadata.is_file() => {
                candidate = root.join(index_file);
                metadata
            }
            _ => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
        },
        _ => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
    };

    let content_type = mime_guess::from_path(&candidate)
        .first_or_octet_stream()
        .to_string();

    if req.method() == Method::HEAD {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, metadata.len())
            .body(Body::empty())
            .unwrap_or_else(internal_response);
    }

    match fs::read(&candidate).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, bytes.len())
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response),
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("file could not be read: {error}"),
        ),
    }
}

pub(super) async fn serve_page(
    req: Request<Body>,
    title: Option<&str>,
    format: PageFormat,
    content: &str,
    remainder: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "page deployments support GET and HEAD\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "page deployments only serve one document\n",
        );
    }

    let html = render_page_content(
        title,
        format,
        content,
        page_theme_from_headers(req.headers()),
    );
    let bytes = html.into_bytes();

    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if req.method() == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

pub(super) async fn serve_project_home(
    req: Request<Body>,
    project: &ProjectConfig,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "project homes support GET and HEAD\n",
        );
    }

    let html = render_project_home(project);
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if req.method() == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

pub(super) async fn serve_project_diff(
    req: Request<Body>,
    project: &ProjectConfig,
    remainder: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::PATCH {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "diff viewers support GET, HEAD, and PATCH\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "diff viewers only serve one document\n",
        );
    }

    if method == Method::PATCH {
        let action_result = handle_git_action_request(req, &project.project_dir).await;
        if let Err(error) = &action_result {
            error!(%error, project = %project.name, "git action failed");
        }

        let report = collect_project_diff(&project.project_dir).await;
        return (
            StatusCode::OK,
            Json(GitActionResponse {
                ok: action_result.is_ok(),
                error: action_result.err(),
                workspace_html: render_diff_workspace_fragment(&report),
            }),
        )
            .into_response();
    }

    let report = collect_project_diff(&project.project_dir).await;
    let html = render_project_diff(project, &report);
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if method == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

pub(super) async fn serve_project_terminal(
    req: Request<Body>,
    project: &ProjectConfig,
    remainder: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::POST {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "terminal viewers support GET, HEAD, and POST\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "terminal viewers only serve one document\n",
        );
    }

    if method == Method::POST {
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

        return Json(execute_terminal_command(&project.project_dir, command).await).into_response();
    }

    let websocket_token = request_bearer_token(&req);
    let info = terminal_info_response(&project.name, &project.project_dir).await;
    let html = render_project_terminal(project, &info, websocket_token.as_deref());
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if method == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

pub(super) async fn serve_server_home(
    req: Request<Body>,
    config: &LatitudeConfig,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "server home supports GET and HEAD\n",
        );
    }

    let html = render_server_home(config);
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if req.method() == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}
