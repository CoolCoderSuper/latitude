mod api;
mod models;
mod serve;

use crate::{config::ProjectConfig, state::AppState};

use super::git::{GitDiffReport, collect_project_diff, collect_project_git_status};

async fn refresh_project_git_statuses(state: &AppState, projects: &[ProjectConfig]) {
    let mut checks = tokio::task::JoinSet::new();
    for project in projects.iter().filter(|project| project.enabled) {
        let name = project.name.clone();
        let project_dir = project.project_dir.clone();
        checks.spawn(async move { (name, collect_project_git_status(&project_dir).await) });
    }

    while let Some(result) = checks.join_next().await {
        if let Ok((name, status)) = result {
            state.set_project_git_status(name, status).await;
        }
    }
}

async fn project_diff_snapshot(state: &AppState, project: &ProjectConfig) -> GitDiffReport {
    if let Some(report) = state.project_git_diff(&project.name).await {
        return (*report).clone();
    }
    let status = state
        .project_git_statuses()
        .await
        .remove(&project.name)
        .unwrap_or_default();
    GitDiffReport {
        repo_dir: project.project_dir.clone(),
        status,
        file_changes: Vec::new(),
    }
}

fn schedule_project_diff_refresh(state: AppState, project: ProjectConfig) {
    if !state.try_begin_project_git_diff_refresh(&project.name) {
        return;
    }
    tokio::spawn(async move {
        let _guard = ProjectDiffRefreshGuard {
            state: state.clone(),
            project: project.name.clone(),
        };
        let report = collect_project_diff(&project.project_dir).await;
        state.set_project_git_diff(project.name, report).await;
    });
}

struct ProjectDiffRefreshGuard {
    state: AppState,
    project: String,
}

impl Drop for ProjectDiffRefreshGuard {
    fn drop(&mut self) {
        self.state.finish_project_git_diff_refresh(&self.project);
    }
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
