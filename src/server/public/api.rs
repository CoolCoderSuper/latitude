mod shares;
mod terminals;

use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, State},
    http::{Method, Request, Response, StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use tracing::error;

use crate::{config::ProjectConfig, state::AppState, storage::WorktreeRecord};

use super::{
    super::{
        auth::{
            clean_next_path, parse_public_login_form, public_auth_set_cookie,
            public_login_next_from_query, public_login_response, public_login_success_response,
            public_password_matches, public_request_is_authenticated,
        },
        constants::{
            AUTH_COOKIE_MAX_AGE_SECONDS, MAX_DIFF_ACTION_PAYLOAD_BYTES, MAX_LOGIN_PAYLOAD_BYTES,
            PUBLIC_API_PROJECTS_PATH,
        },
        git::{
            GitAction, GitCommandExecution, PublicGitActionResponse, collect_project_diff,
            collect_project_git_commit, collect_project_git_history, execute_git_action,
            parse_public_git_action_payload, public_commit_response, public_diff_response,
            public_history_response,
        },
        response::{ApiError, json_error, plain_response},
    },
    models::{
        PublicLoginPayload, PublicLoginResponse, PublicProjectListResponse, PublicSessionResponse,
        public_project_detail, public_project_summary, public_root_desktop_link,
        public_root_terminal_link,
    },
};

pub(in crate::server) use shares::{
    public_api_create_share, public_api_delete_share, public_api_list_shares,
    public_ui_create_share, public_ui_delete_share, public_ui_get_shares,
};
pub(in crate::server) use terminals::{
    public_api_create_root_terminal_session, public_api_create_terminal_session,
    public_api_delete_root_terminal_session, public_api_delete_terminal_session,
    public_api_get_project_terminal, public_api_get_root_terminal,
    public_api_list_root_terminal_sessions, public_api_list_terminal_sessions,
    public_api_post_project_terminal, public_api_post_root_terminal, public_root_terminal_ws,
    public_terminal_ws,
};

#[cfg(test)]
pub(in crate::server) use shares::ShareUiForm;

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
    super::refresh_git_snapshot(
        &state,
        request_fetches_remote(&req),
        super::INTERACTIVE_GIT_SNAPSHOT_MAX_AGE,
        super::git_command_execution(req.uri().query()),
    )
    .await;
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

pub(super) async fn refresh_project_list(
    state: &AppState,
    fetch_remote: bool,
    project_name: Option<&str>,
    execution: GitCommandExecution,
) {
    let Ok(projects) = state.catalog().list_projects().await else {
        error!("project list could not be loaded for Git refresh");
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
            project.enabled
                && project_is_in_refresh_scope(&project.name, project_name)
                && project_needs_git_status(&project.name, &worktrees_by_project)
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

    while fetches.join_next().await.is_some() {}
    super::refresh_project_git_statuses(state, &status_projects, execution).await;
}

pub(in crate::server) async fn public_ui_archive_project(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
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
    super::refresh_project_git_snapshot(
        &state,
        &project,
        request_fetches_remote(&req),
        super::git_command_execution(req.uri().query()),
    )
    .await;
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };

    let git_status = state
        .project_git_statuses()
        .await
        .remove(&project_config.name)
        .unwrap_or_default();
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

fn project_is_in_refresh_scope(project: &str, requested_project: Option<&str>) -> bool {
    requested_project.is_none_or(|requested_project| project == requested_project)
}

pub(in crate::server) async fn public_api_get_project_diff(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };

    let report = collect_project_diff(&project_config.project_dir).await;
    Json(public_diff_response(report)).into_response()
}

pub(in crate::server) async fn public_api_get_project_git_history(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
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
) -> Response<Body> {
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
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
    let project_config = match enabled_project_or_response(&state, &project).await {
        Ok(project) => project,
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
    Json(PublicGitActionResponse {
        ok: action_result.is_ok(),
        error: action_result.err(),
        diff: public_diff_response(diff),
    })
    .into_response()
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

pub(super) async fn enabled_project_or_response(
    state: &AppState,
    project: &str,
) -> Result<ProjectConfig, Response<Body>> {
    match state.catalog().get_project(project).await {
        Ok(Some(project)) if project.enabled => Ok(project),
        Ok(_) => Err(json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        )),
        Err(error) => {
            error!(%error, project, "project lookup failed");
            Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog could not be read",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use crate::storage::WorktreeRecord;

    use super::{project_is_in_refresh_scope, project_needs_git_status};

    #[test]
    fn limits_project_refresh_to_the_requested_project() {
        assert!(project_is_in_refresh_scope("requested", None));
        assert!(project_is_in_refresh_scope("unrelated", None));
        assert!(project_is_in_refresh_scope("requested", Some("requested")));
        assert!(!project_is_in_refresh_scope("unrelated", Some("requested")));
    }

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
