mod api;
mod models;
mod serve;

use crate::config::ProjectConfig;
use std::collections::HashMap;

use super::git::{GitStatusSummary, collect_project_git_status};

async fn project_git_statuses(projects: &[ProjectConfig]) -> HashMap<String, GitStatusSummary> {
    let mut checks = tokio::task::JoinSet::new();
    for project in projects.iter().filter(|project| project.enabled) {
        let name = project.name.clone();
        let project_dir = project.project_dir.clone();
        checks.spawn(async move { (name, collect_project_git_status(&project_dir).await) });
    }

    let mut statuses = HashMap::new();
    while let Some(result) = checks.join_next().await {
        if let Ok((name, status)) = result {
            statuses.insert(name, status);
        }
    }
    statuses
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
    public_api_session, public_root_terminal_ws, public_terminal_ws, public_ui_create_share,
    public_ui_delete_share, public_ui_get_shares,
};
pub(super) use serve::public_entry;

#[cfg(test)]
pub(super) use models::public_project_detail;
