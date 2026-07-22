use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Form, Path as AxumPath, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Method, Request, Response, StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicBool, Ordering},
};
use tracing::error;

use crate::{
    config::{ProjectConfig, current_unix_timestamp},
    state::AppState,
    storage::WorktreeRecord,
};

use super::{
    super::{
        auth::{
            clean_next_path, parse_public_login_form, public_api_auth_challenge,
            public_auth_set_cookie, public_headers_are_authenticated, public_login_next_from_query,
            public_login_response, public_login_success_response, public_password_matches,
            public_request_is_authenticated,
        },
        command::{CreateDeploymentShareRequest, deployment_share_response},
        constants::{
            AUTH_COOKIE_MAX_AGE_SECONDS, MAX_DIFF_ACTION_PAYLOAD_BYTES, MAX_LOGIN_PAYLOAD_BYTES,
            MAX_TERMINAL_COMMAND_BYTES, PUBLIC_API_PROJECTS_PATH,
        },
        git::{
            GitAction, PublicGitActionResponse, collect_project_diff, collect_project_git_commit,
            collect_project_git_history, discover_worktrees, execute_git_action,
            parse_public_git_action_payload, public_commit_response, public_diff_response,
            public_history_response,
        },
        render::render_share_dialog_shell,
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

#[derive(Debug, Deserialize)]
pub(in crate::server) struct ShareUiForm {
    #[serde(default)]
    pub(in crate::server) password: Option<String>,
    #[serde(default)]
    pub(in crate::server) expiry: Option<u64>,
}

pub(in crate::server) async fn public_ui_get_shares(
    AxumPath((project, deployment)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, None) {
        return public_api_auth_challenge();
    }

    render_share_ui_response(&state, &project, &deployment, None).await
}

pub(in crate::server) async fn public_ui_create_share(
    AxumPath((project, deployment)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<ShareUiForm>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, None) {
        return public_api_auth_challenge();
    }

    if let Err(error) = enabled_deployment(&state, &project, &deployment).await {
        return render_share_ui_response(&state, &project, &deployment, Some((&error, true))).await;
    }

    let password = payload
        .password
        .map(|password| password.trim().to_string())
        .filter(|password| !password.is_empty());
    let expires_at = payload
        .expiry
        .filter(|seconds| *seconds > 0)
        .map(|seconds| current_unix_timestamp().saturating_add(seconds));
    let result = state
        .catalog()
        .create_share(&project, &deployment, password, expires_at)
        .await;

    match result {
        Ok(_) => {
            render_share_ui_response(
                &state,
                &project,
                &deployment,
                Some(("Share link created.", false)),
            )
            .await
        }
        Err(error) => {
            let message = error.to_string();
            render_share_ui_response(&state, &project, &deployment, Some((&message, true))).await
        }
    }
}

pub(in crate::server) async fn public_ui_delete_share(
    AxumPath((project, deployment, token)): AxumPath<(String, String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, None) {
        return public_api_auth_challenge();
    }

    let result = match state.catalog().get_share(&token).await {
        Ok(Some(share)) if share.project == project && share.deployment == deployment => {
            state.catalog().delete_share(&token).await
        }
        Ok(_) => Ok(false),
        Err(error) => Err(error),
    };

    match result {
        Ok(true) => {
            render_share_ui_response(
                &state,
                &project,
                &deployment,
                Some(("Share link revoked.", false)),
            )
            .await
        }
        Ok(false) => {
            render_share_ui_response(
                &state,
                &project,
                &deployment,
                Some(("Share link was not found.", true)),
            )
            .await
        }
        Err(error) => {
            let message = error.to_string();
            render_share_ui_response(&state, &project, &deployment, Some((&message, true))).await
        }
    }
}

async fn enabled_deployment(
    state: &AppState,
    project: &str,
    deployment: &str,
) -> Result<(), String> {
    let project_config = state
        .catalog()
        .get_project(project)
        .await
        .map_err(|error| error.to_string())?
        .filter(|project| project.enabled)
        .ok_or_else(|| format!("project '{project}' was not found"))?;

    project_config
        .deployments
        .iter()
        .any(|candidate| candidate.enabled && candidate.name == deployment)
        .then_some(())
        .ok_or_else(|| format!("deployment '{deployment}' was not found"))
}

async fn render_share_ui_response(
    state: &AppState,
    project: &str,
    deployment: &str,
    status: Option<(&str, bool)>,
) -> Response<Body> {
    let shares = match state.catalog().list_shares().await {
        Ok(shares) => shares,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let html = render_share_dialog_shell(project, deployment, &shares, status).into_string();
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

pub(in crate::server) async fn public_api_list_shares(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    match state.catalog().list_shares().await {
        Ok(shares) => {
            let now = current_unix_timestamp();
            Json(
                shares
                    .iter()
                    .map(|share| deployment_share_response(share, now))
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub(in crate::server) async fn public_api_create_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateDeploymentShareRequest>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, None) {
        return public_api_auth_challenge();
    }

    match state
        .catalog()
        .create_share(
            &payload.project,
            &payload.deployment,
            payload.password,
            payload.expires_at,
        )
        .await
    {
        Ok(share) => (
            StatusCode::CREATED,
            Json(deployment_share_response(&share, current_unix_timestamp())),
        )
            .into_response(),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub(in crate::server) async fn public_api_delete_share(
    AxumPath(token): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    match state.catalog().delete_share(&token).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => json_error(
            StatusCode::NOT_FOUND,
            format!("share link '{token}' was not found"),
        ),
        Err(error) => ApiError::from(error).into_response(),
    }
}

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

    discover_worktrees(&state).await;
    let catalog_projects = match list_catalog_projects_or_response(&state).await {
        Ok(projects) => projects,
        Err(response) => return response,
    };
    let worktrees = match state.catalog().list_worktrees().await {
        Ok(worktrees) => worktrees,
        Err(error) => {
            error!(%error, "worktree metadata list failed");
            Vec::new()
        }
    };
    let worktrees_by_project = worktrees
        .iter()
        .map(|worktree| (worktree.project_name.as_str(), worktree))
        .collect::<HashMap<_, _>>();
    let git_statuses = state.project_git_statuses().await;
    schedule_project_list_refresh(state.clone(), request_fetches_remote(&req));
    let projects = catalog_projects
        .iter()
        .filter(|project| project.enabled)
        .map(|project| {
            let status = if project_needs_git_status(&project.name, &worktrees_by_project) {
                git_statuses.get(&project.name).cloned().unwrap_or_default()
            } else {
                Default::default()
            };
            public_project_summary(
                project,
                &status,
                worktrees_by_project.get(project.name.as_str()).copied(),
            )
        })
        .collect();

    Json(PublicProjectListResponse {
        device_hostname: state.device_hostname().to_string(),
        root_terminal: public_root_terminal_link(),
        root_desktop: public_root_desktop_link(&config.desktop),
        projects,
    })
    .into_response()
}

#[derive(Debug, Deserialize)]
struct WorktreeArchivePayload {
    archived: bool,
}

pub(in crate::server) async fn public_api_patch_project_archive(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }
    let body = match to_bytes(req.into_body(), MAX_LOGIN_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let payload: WorktreeArchivePayload = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error.to_string()),
    };
    match state
        .catalog()
        .set_worktree_archived(&project, payload.archived)
        .await
    {
        Ok(true) => Json(serde_json::json!({ "ok": true })).into_response(),
        Ok(false) => json_error(
            StatusCode::NOT_FOUND,
            format!("worktree project '{project}' was not found"),
        ),
        Err(error) => {
            error!(%error, project = %project, "worktree archive update failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "archive state could not be updated",
            )
        }
    }
}

static PROJECT_LIST_REFRESH_RUNNING: AtomicBool = AtomicBool::new(false);

pub(super) fn schedule_project_list_refresh(state: AppState, fetch_remote: bool) {
    if PROJECT_LIST_REFRESH_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    tokio::spawn(async move {
        let _guard = ProjectListRefreshGuard;
        let Ok(projects) = state.catalog().list_projects().await else {
            return;
        };
        let worktrees = state.catalog().list_worktrees().await.unwrap_or_default();
        let worktrees_by_project = worktrees
            .iter()
            .map(|worktree| (worktree.project_name.as_str(), worktree))
            .collect::<HashMap<_, _>>();
        let status_projects = projects
            .into_iter()
            .filter(|project| {
                project.enabled && project_needs_git_status(&project.name, &worktrees_by_project)
            })
            .collect::<Vec<_>>();

        let mut fetches = tokio::task::JoinSet::new();
        if fetch_remote {
            let mut repositories = HashSet::new();
            for project in &status_projects {
                let repository = worktrees_by_project
                    .get(project.name.as_str())
                    .map(|worktree| {
                        worktree
                            .common_git_dir
                            .to_string_lossy()
                            .to_ascii_lowercase()
                    })
                    .unwrap_or_else(|| project.project_dir.to_string_lossy().to_ascii_lowercase());
                if repositories.insert(repository) {
                    let project_dir = project.project_dir.clone();
                    fetches.spawn(async move {
                        let _ = execute_git_action(&project_dir, GitAction::Fetch).await;
                    });
                }
            }
        }

        super::refresh_project_git_statuses(&state, &status_projects).await;
        while fetches.join_next().await.is_some() {}
    });
}

struct ProjectListRefreshGuard;

impl Drop for ProjectListRefreshGuard {
    fn drop(&mut self) {
        PROJECT_LIST_REFRESH_RUNNING.store(false, Ordering::Release);
    }
}

pub(in crate::server) async fn public_ui_archive_project(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }
    match state.catalog().set_worktree_archived(&project, true).await {
        Ok(true) => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header("HX-Trigger", "worktreeArchived")
            .body(Body::empty())
            .expect("HTMX archive response"),
        Ok(false) => json_error(
            StatusCode::NOT_FOUND,
            format!("worktree project '{project}' was not found"),
        ),
        Err(error) => {
            error!(%error, project = %project, "worktree archive update failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "archive state could not be updated",
            )
        }
    }
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

    let git_status = state
        .project_git_statuses()
        .await
        .remove(&project_config.name)
        .unwrap_or_default();
    schedule_project_list_refresh(state.clone(), request_fetches_remote(&req));
    Json(public_project_detail(
        &project_config,
        &git_status,
        state.device_hostname(),
    ))
    .into_response()
}

fn request_fetches_remote(req: &Request<Body>) -> bool {
    req.uri().query().is_some_and(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .any(|(key, value)| key == "fetch" && matches!(value.as_ref(), "1" | "true"))
    })
}

fn project_needs_git_status(
    project: &str,
    worktrees_by_project: &HashMap<&str, &WorktreeRecord>,
) -> bool {
    !worktrees_by_project
        .get(project)
        .is_some_and(|worktree| worktree.archived)
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

    let report = super::project_diff_snapshot(&state, &project_config).await;
    super::schedule_project_diff_refresh(state, project_config);
    Json(public_diff_response(report)).into_response()
}

pub(in crate::server) async fn public_api_get_project_git_history(
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
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "project was not found"),
        Err(response) => return response,
    };
    Json(public_history_response(
        collect_project_git_history(&project_config.project_dir).await,
    ))
    .into_response()
}

pub(in crate::server) async fn public_api_get_project_git_commit(
    AxumPath((project, hash)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(Some(project)) => project,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "project was not found"),
        Err(response) => return response,
    };
    let Some(report) = collect_project_git_commit(&project_config.project_dir, &hash).await else {
        return json_error(StatusCode::NOT_FOUND, "commit was not found");
    };
    Json(public_commit_response(report)).into_response()
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

    let diff = collect_project_diff(&project_config.project_dir).await;
    state
        .set_project_git_diff(project_config.name.clone(), diff.clone())
        .await;
    Json(PublicGitActionResponse {
        ok: action_result.is_ok(),
        error: action_result.err(),
        diff: public_diff_response(diff),
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use crate::storage::WorktreeRecord;

    use super::project_needs_git_status;

    #[test]
    fn skips_git_status_for_archived_worktrees() {
        let active = WorktreeRecord {
            project_name: "active".to_string(),
            common_git_dir: PathBuf::from("C:/repo/.git"),
            worktree_dir: PathBuf::from("C:/repo-active"),
            branch: Some("active".to_string()),
            head: "abc123".to_string(),
            discovered: true,
            archived: false,
        };
        let archived = WorktreeRecord {
            project_name: "archived".to_string(),
            common_git_dir: PathBuf::from("C:/repo/.git"),
            worktree_dir: PathBuf::from("C:/repo-archived"),
            branch: Some("archived".to_string()),
            head: "def456".to_string(),
            discovered: true,
            archived: true,
        };
        let worktrees = HashMap::from([
            (active.project_name.as_str(), &active),
            (archived.project_name.as_str(), &archived),
        ]);

        assert!(project_needs_git_status("active", &worktrees));
        assert!(!project_needs_git_status("archived", &worktrees));
        assert!(project_needs_git_status("not-a-worktree", &worktrees));
    }
}
