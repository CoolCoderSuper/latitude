mod api;
mod models;
mod serve;

use crate::{
    config::ProjectConfig,
    state::{AppState, GitRefreshAccess},
};
use tracing::{debug, warn};

use super::git::{collect_project_git_status, discover_worktrees};

const INTERACTIVE_GIT_SNAPSHOT_MAX_AGE: std::time::Duration = std::time::Duration::from_millis(500);
const AUTO_REFRESH_GIT_SNAPSHOT_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(10);

async fn refresh_project_git_statuses(state: &AppState, projects: &[ProjectConfig]) {
    let concurrency = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut checks = tokio::task::JoinSet::new();
    for project in projects.iter().filter(|project| project.enabled) {
        let name = project.name.clone();
        let project_dir = project.project_dir.clone();
        let concurrency = concurrency.clone();
        checks.spawn(async move {
            let Ok(_permit) = concurrency.acquire_owned().await else {
                return None;
            };
            Some((name, collect_project_git_status(&project_dir).await))
        });
    }

    while let Some(result) = checks.join_next().await {
        match result {
            Ok(Some((name, status))) => state.set_project_git_status(name, status).await,
            Ok(None) => {}
            Err(error) => warn!(%error, "Git status refresh task failed"),
        }
    }
}

async fn refresh_git_snapshot(
    state: &AppState,
    fetch_remote: bool,
    max_snapshot_age: std::time::Duration,
) {
    let GitRefreshAccess::Leader(permit) = state
        .acquire_git_refresh(fetch_remote, max_snapshot_age)
        .await
    else {
        return;
    };
    let started = std::time::Instant::now();
    discover_worktrees(state).await;
    api::refresh_project_list(state, fetch_remote).await;
    permit.complete();
    debug!(
        elapsed_ms = started.elapsed().as_millis(),
        fetch_remote, "request-time Git snapshot refreshed"
    );
}

#[cfg(test)]
pub(super) use api::ShareUiForm;
pub(super) use api::{
    get_public_login, post_public_login, public_api_create_root_terminal_session,
    public_api_create_share, public_api_create_terminal_session,
    public_api_delete_root_terminal_session, public_api_delete_share,
    public_api_delete_terminal_session, public_api_get_project, public_api_get_project_diff,
    public_api_get_project_git_commit, public_api_get_project_git_history,
    public_api_get_project_terminal, public_api_get_root_terminal, public_api_list_projects,
    public_api_list_root_terminal_sessions, public_api_list_shares,
    public_api_list_terminal_sessions, public_api_login, public_api_patch_project_archive,
    public_api_patch_project_diff, public_api_post_project_terminal, public_api_post_root_terminal,
    public_api_session, public_root_terminal_ws, public_terminal_ws, public_ui_archive_project,
    public_ui_create_share, public_ui_delete_share, public_ui_get_shares,
};
pub(super) use serve::public_entry;

#[cfg(test)]
pub(super) use models::public_project_detail;
