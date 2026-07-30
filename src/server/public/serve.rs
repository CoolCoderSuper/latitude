use axum::{
    Json,
    body::{Body, to_bytes},
    extract::State,
    http::{Method, Request, Response, StatusCode, header},
    response::IntoResponse,
};
use tracing::error;

use crate::{
    config::{BootConfig, DeploymentShareConfig, ProjectConfig, current_unix_timestamp},
    desktop::desktop_info_response,
    state::AppState,
};

use super::super::{
    auth::{
        clean_next_path, header_cookie_value, parse_public_login_form, public_auth_challenge,
        public_login_success_response, public_password_matches, public_request_is_authenticated,
        request_bearer_token,
    },
    constants::{
        AUTH_COOKIE_MAX_AGE_SECONDS, DESKTOP_ROUTE_SEGMENT, DIFF_ROUTE_SEGMENT,
        FILES_ROUTE_SEGMENT, MAX_LOGIN_PAYLOAD_BYTES, MAX_TERMINAL_COMMAND_BYTES,
        PUBLIC_ROOT_DESKTOP_WS_PATH, PUBLIC_SHARE_BASE_PATH, TERMINAL_ROUTE_SEGMENT,
    },
    desktop_api::execute_desktop_action_request,
    git::{
        GitCommandExecution, collect_project_diff, collect_project_file_diff,
        collect_project_git_commit, collect_project_git_history, handle_git_action_request,
    },
    paths::{ProjectPath, split_project_path},
    render::{
        render_diff_file_update, render_diff_workspace_fragment, render_project_diff,
        render_project_files, render_project_git_commit, render_project_git_history,
        render_project_home, render_project_terminal, render_root_desktop, render_root_terminal,
        render_server_home, render_share_login,
    },
    response::{html_response, html_status_response, json_error, plain_response},
    terminal_api::{
        execute_root_terminal_command, execute_terminal_command, parse_terminal_command_payload,
        root_terminal_info_response, terminal_info_response,
    },
};

use super::deployment::{DeploymentRequest, serve_deployment_target};

pub(in crate::server) async fn public_entry(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let original_path = req.uri().path().to_string();
    let config = state.config_snapshot().await;
    let device_hostname = state.device_hostname().to_string();

    if original_path == PUBLIC_SHARE_BASE_PATH
        || original_path.starts_with(&format!("{PUBLIC_SHARE_BASE_PATH}/"))
    {
        let Some(share_path) = split_share_path(&original_path) else {
            return plain_response(StatusCode::NOT_FOUND, "share link was not found\n");
        };
        return serve_shared_deployment(state, req, share_path, &device_hostname).await;
    }

    if !public_request_is_authenticated(&state, &config, &req) {
        return public_auth_challenge(&state, &req, false);
    }

    if original_path == "/" {
        return serve_server_home(req, &state, &config, &device_hostname).await;
    }

    if let Some(remainder) = root_terminal_remainder(&original_path) {
        return serve_root_terminal(req, remainder, &device_hostname).await;
    }

    if let Some(remainder) = root_desktop_remainder(&original_path) {
        return serve_root_desktop(req, &state, &config, remainder, &device_hostname).await;
    }

    let Some(public_path) = split_project_path(&original_path) else {
        return plain_response(
            StatusCode::NOT_FOUND,
            "Latitude is running. Mount a deployment at /{project}/{name} to serve traffic.\n",
        );
    };
    let project_mount = public_path.project_name().to_string();

    let project = match load_enabled_project(&state, &project_mount).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return plain_response(
                StatusCode::NOT_FOUND,
                format!("No enabled project is mounted at /{project_mount}\n"),
            );
        }
        Err(response) => return response,
    };

    let ProjectPath::Deployment {
        deployment: app_mount,
        remainder,
        ..
    } = public_path
    else {
        return serve_project_home(
            req,
            &state,
            &project,
            config.t3code.enabled,
            &device_hostname,
        )
        .await;
    };

    if app_mount == DIFF_ROUTE_SEGMENT {
        return serve_project_diff(req, &project, remainder.as_str(), &device_hostname).await;
    }
    if app_mount == FILES_ROUTE_SEGMENT {
        if remainder.as_str() != "/" {
            return plain_response(
                StatusCode::NOT_FOUND,
                "file viewer only serves one document\n",
            );
        }
        if req.method() != Method::GET && req.method() != Method::HEAD {
            return plain_response(
                StatusCode::METHOD_NOT_ALLOWED,
                "file viewer supports GET and HEAD\n",
            );
        }
        return html_response(
            req.method(),
            render_project_files(&project, &device_hostname),
        );
    }
    if app_mount == TERMINAL_ROUTE_SEGMENT {
        return serve_project_terminal(req, &project, remainder.as_str(), &device_hostname).await;
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

    let mount_path = format!("/{}/{}", project.name, app.name);
    serve_deployment_target(
        state,
        req,
        DeploymentRequest {
            project: &project,
            deployment: &app,
            remainder: remainder.as_str(),
            mount_path: &mount_path,
            extra_excluded_cookie_name: None,
            device_hostname: &device_hostname,
        },
    )
    .await
}

struct SharePath {
    token: String,
    mount_path: String,
    remainder: String,
}

async fn serve_shared_deployment(
    state: AppState,
    req: Request<Body>,
    share_path: SharePath,
    device_hostname: &str,
) -> Response<Body> {
    let share = match state.catalog().get_share(&share_path.token).await {
        Ok(Some(share)) => share,
        Ok(None) => return plain_response(StatusCode::NOT_FOUND, "share link was not found\n"),
        Err(error) => {
            error!(%error, token = %share_path.token, "share link lookup failed");
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog could not be read\n",
            );
        }
    };

    let now = current_unix_timestamp();
    if share.is_expired(now) {
        return plain_response(StatusCode::GONE, "share link has expired\n");
    }

    if !share_request_is_authenticated(&state, &req, &share) {
        if req.method() == Method::POST {
            return handle_share_login_post(state, req, &share, &share_path, now, device_hostname)
                .await;
        }
        return share_auth_challenge(&req, &share_path, false, device_hostname);
    }

    let project = match load_enabled_project(&state, &share.project).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return plain_response(
                StatusCode::NOT_FOUND,
                format!("No enabled project is mounted at {}\n", share.project),
            );
        }
        Err(response) => return response,
    };

    let Some(app) = project
        .deployments
        .iter()
        .find(|app| app.enabled && app.name == share.deployment)
        .cloned()
    else {
        return plain_response(
            StatusCode::NOT_FOUND,
            format!(
                "No enabled deployment is mounted at {}/{}\n",
                share.project, share.deployment
            ),
        );
    };

    let share_cookie_name = share
        .password
        .as_ref()
        .map(|_| share_auth_cookie_name(&share.token));
    serve_deployment_target(
        state,
        req,
        DeploymentRequest {
            project: &project,
            deployment: &app,
            remainder: &share_path.remainder,
            mount_path: &share_path.mount_path,
            extra_excluded_cookie_name: share_cookie_name.as_deref(),
            device_hostname,
        },
    )
    .await
}

async fn handle_share_login_post(
    state: AppState,
    req: Request<Body>,
    share: &DeploymentShareConfig,
    share_path: &SharePath,
    now: u64,
    device_hostname: &str,
) -> Response<Body> {
    let query_next = req
        .uri()
        .path_and_query()
        .map(|path_and_query| path_and_query.as_str().to_string());
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_LOGIN_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return plain_response(
                StatusCode::BAD_REQUEST,
                format!("share login payload could not be read: {error}\n"),
            );
        }
    };

    let form = parse_public_login_form(&body);
    let next = clean_next_path(form.next.or(query_next));

    if share
        .password
        .as_deref()
        .is_some_and(|password| public_password_matches(&form.password, password))
    {
        return public_login_success_response(&next, share_auth_set_cookie(&state, share, now));
    }

    share_auth_challenge_for_path(&share_path.mount_path, &next, true, false, device_hostname)
}

fn share_auth_challenge(
    req: &Request<Body>,
    share_path: &SharePath,
    login_failed: bool,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(StatusCode::UNAUTHORIZED, "share password required\n");
    }

    let next = clean_next_path(
        req.uri()
            .path_and_query()
            .map(|path_and_query| path_and_query.as_str().to_string()),
    );

    share_auth_challenge_for_path(
        &share_path.mount_path,
        &next,
        login_failed,
        req.method() == Method::HEAD,
        device_hostname,
    )
}

fn share_auth_challenge_for_path(
    action: &str,
    next: &str,
    login_failed: bool,
    head: bool,
    device_hostname: &str,
) -> Response<Body> {
    let method = if head { Method::HEAD } else { Method::GET };
    html_status_response(
        StatusCode::UNAUTHORIZED,
        &method,
        render_share_login(action, next, login_failed, device_hostname),
    )
}

fn share_request_is_authenticated(
    state: &AppState,
    req: &Request<Body>,
    share: &DeploymentShareConfig,
) -> bool {
    if share.password.is_none() {
        return true;
    }

    header_cookie_value(req.headers(), &share_auth_cookie_name(&share.token))
        .as_deref()
        .is_some_and(|value| state.verify_public_auth_cookie(&share_auth_key(share), value))
}

fn share_auth_set_cookie(state: &AppState, share: &DeploymentShareConfig, now: u64) -> String {
    let value = state.public_auth_cookie_value(&share_auth_key(share));
    let cookie_name = share_auth_cookie_name(&share.token);
    let max_age = share
        .expires_at
        .map(|expires_at| expires_at.saturating_sub(now))
        .filter(|seconds| *seconds > 0)
        .unwrap_or(AUTH_COOKIE_MAX_AGE_SECONDS)
        .min(AUTH_COOKIE_MAX_AGE_SECONDS);
    format!(
        "{cookie_name}={value}; HttpOnly; SameSite=Lax; Path={PUBLIC_SHARE_BASE_PATH}/{}; Max-Age={max_age}",
        share.token
    )
}

fn share_auth_key(share: &DeploymentShareConfig) -> String {
    format!(
        "share:{}:{}:{}:{}",
        share.token,
        share.project,
        share.deployment,
        share.password.as_deref().unwrap_or("")
    )
}

fn share_auth_cookie_name(token: &str) -> String {
    format!("latitude_share_{token}")
}

fn split_share_path(path: &str) -> Option<SharePath> {
    let prefix = format!("{PUBLIC_SHARE_BASE_PATH}/");
    let rest = path.strip_prefix(&prefix)?;
    let mut segments = rest.splitn(2, '/');
    let token = segments.next()?.to_string();
    if token.is_empty() {
        return None;
    }

    let remainder = segments
        .next()
        .map(|rest| format!("/{rest}"))
        .unwrap_or_else(|| "/".to_string());

    Some(SharePath {
        mount_path: format!("{PUBLIC_SHARE_BASE_PATH}/{token}"),
        token,
        remainder,
    })
}

async fn serve_project_home(
    req: Request<Body>,
    state: &AppState,
    project: &ProjectConfig,
    t3code_enabled: bool,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "project homes support GET and HEAD\n",
        );
    }

    super::refresh_project_git_snapshot(
        state,
        &project.name,
        false,
        GitCommandExecution::Interactive,
    )
    .await;
    let git_status = state
        .project_git_statuses()
        .await
        .remove(&project.name)
        .unwrap_or_default();
    html_response(
        req.method(),
        render_project_home(project, &git_status, t3code_enabled, device_hostname),
    )
}

async fn serve_project_diff(
    req: Request<Body>,
    project: &ProjectConfig,
    remainder: &str,
    device_hostname: &str,
) -> Response<Body> {
    let method = req.method().clone();
    let is_htmx_request = req.headers().contains_key("hx-request");
    if method != Method::GET && method != Method::HEAD && method != Method::PATCH {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "diff viewers support GET, HEAD, and PATCH\n",
        );
    }

    if remainder == "/history" && (method == Method::GET || method == Method::HEAD) {
        let report = collect_project_git_history(&project.project_dir).await;
        return html_response(
            &method,
            render_project_git_history(project, &report, device_hostname),
        );
    }

    if let Some(hash) = remainder.strip_prefix("/history/")
        && (method == Method::GET || method == Method::HEAD)
    {
        let Some(report) = collect_project_git_commit(&project.project_dir, hash).await else {
            return plain_response(StatusCode::NOT_FOUND, "commit was not found\n");
        };
        return html_response(
            &method,
            render_project_git_commit(project, &report, device_hostname),
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "diff viewers only serve one document\n",
        );
    }

    if method == Method::PATCH {
        let action = match handle_git_action_request(req, &project.project_dir).await {
            Ok(action) => action,
            Err(error) => {
                error!(%error, project = %project.name, "git action failed");
                return plain_response(StatusCode::UNPROCESSABLE_ENTITY, error);
            }
        };
        let Some(path) = action.affected_path() else {
            return StatusCode::NO_CONTENT.into_response();
        };
        let report = collect_project_file_diff(&project.project_dir, path).await;
        return html_response(
            &method,
            render_diff_file_update(
                &report,
                path,
                &format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT),
            )
            .into_string(),
        );
    }

    let report = collect_project_diff(&project.project_dir).await;
    if is_htmx_request && method == Method::GET {
        return html_response(
            &method,
            render_diff_workspace_fragment(
                &report,
                &format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT),
            )
            .into_string(),
        );
    }
    html_response(
        &method,
        render_project_diff(project, &report, device_hostname),
    )
}

async fn serve_project_terminal(
    req: Request<Body>,
    project: &ProjectConfig,
    remainder: &str,
    device_hostname: &str,
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
    html_response(
        &method,
        render_project_terminal(project, &info, websocket_token.as_deref(), device_hostname),
    )
}

async fn serve_root_terminal(
    req: Request<Body>,
    remainder: &str,
    device_hostname: &str,
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

        return Json(execute_root_terminal_command(command).await).into_response();
    }

    let websocket_token = request_bearer_token(&req);
    let info = root_terminal_info_response().await;
    html_response(
        &method,
        render_root_terminal(&info, websocket_token.as_deref(), device_hostname),
    )
}

async fn serve_root_desktop(
    req: Request<Body>,
    state: &AppState,
    config: &BootConfig,
    remainder: &str,
    device_hostname: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::PATCH {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "desktop viewers support GET, HEAD, and PATCH\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "desktop viewers only serve one document\n",
        );
    }

    if !config.desktop.enabled {
        return plain_response(StatusCode::NOT_FOUND, "desktop is not enabled\n");
    }

    if method == Method::PATCH {
        return execute_desktop_action_request(req).await;
    }

    let target = match state.desktop_manager().target_for(&config.desktop).await {
        Ok(target) => target,
        Err(error) => {
            return plain_response(
                StatusCode::BAD_GATEWAY,
                format!("desktop target could not be prepared: {error}\n"),
            );
        }
    };

    let websocket_token = request_bearer_token(&req);
    let info = desktop_info_response(
        &config.desktop,
        &target,
        PUBLIC_ROOT_DESKTOP_WS_PATH.to_string(),
    );
    html_response(
        &method,
        render_root_desktop(&info, websocket_token.as_deref(), device_hostname),
    )
}

async fn serve_server_home(
    req: Request<Body>,
    state: &AppState,
    config: &BootConfig,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "server home supports GET and HEAD\n",
        );
    }

    let git_execution = super::git_command_execution(req.uri().query());
    let git_snapshot_max_age = if git_execution == GitCommandExecution::AutoRefresh {
        super::AUTO_REFRESH_GIT_SNAPSHOT_MAX_AGE
    } else {
        super::INTERACTIVE_GIT_SNAPSHOT_MAX_AGE
    };
    super::refresh_git_snapshot(state, false, git_snapshot_max_age, git_execution).await;
    let is_htmx_refresh = req
        .headers()
        .get("HX-Request")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let mut projects = match state.catalog().list_projects().await {
        Ok(projects) => projects,
        Err(error) => {
            error!(%error, "project list failed");
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog could not be read\n",
            );
        }
    };
    let worktrees = state.catalog().list_worktrees().await.unwrap_or_default();
    if !worktrees.is_empty() {
        let archived = worktrees
            .iter()
            .filter(|worktree| worktree.archived)
            .map(|worktree| worktree.project_name.clone())
            .collect::<std::collections::HashSet<_>>();
        projects.retain(|project| !archived.contains(&project.name));
    }
    let git_statuses = if is_htmx_refresh {
        std::collections::HashMap::new()
    } else {
        state.project_git_statuses().await
    };

    html_response(
        req.method(),
        render_server_home(
            config,
            &projects,
            &git_statuses,
            &worktrees,
            device_hostname,
        ),
    )
}

async fn load_enabled_project(
    state: &AppState,
    name: &str,
) -> Result<Option<ProjectConfig>, Response<Body>> {
    state
        .catalog()
        .get_project(name)
        .await
        .map(|project| project.filter(|project| project.enabled))
        .map_err(|error| {
            error!(%error, project = %name, "project lookup failed");
            plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog could not be read\n",
            )
        })
}

fn root_terminal_remainder(path: &str) -> Option<&str> {
    let root_terminal_path = format!("/{TERMINAL_ROUTE_SEGMENT}");
    if path == root_terminal_path {
        return Some("/");
    }

    let root_terminal_prefix = format!("{root_terminal_path}/");
    path.strip_prefix(&root_terminal_prefix).map(
        |remainder| {
            if remainder.is_empty() { "/" } else { remainder }
        },
    )
}

fn root_desktop_remainder(path: &str) -> Option<&str> {
    let root_desktop_path = format!("/{DESKTOP_ROUTE_SEGMENT}");
    if path == root_desktop_path {
        return Some("/");
    }

    let root_desktop_prefix = format!("{root_desktop_path}/");
    path.strip_prefix(&root_desktop_prefix).map(
        |remainder| {
            if remainder.is_empty() { "/" } else { remainder }
        },
    )
}
