mod api;
mod models;
mod serve;

use crate::config::ProjectConfig;

use super::git::project_is_dirty;

async fn dirty_project_names(projects: &[ProjectConfig]) -> Vec<String> {
    let mut checks = tokio::task::JoinSet::new();
    for project in projects.iter().filter(|project| project.enabled) {
        let name = project.name.clone();
        let project_dir = project.project_dir.clone();
        checks.spawn(async move { (name, project_is_dirty(&project_dir).await) });
    }

    let mut dirty = Vec::new();
    while let Some(result) = checks.join_next().await {
        if let Ok((name, true)) = result {
            dirty.push(name);
        }
    }
    dirty
}

pub(super) use api::{
    get_public_login, post_public_login, public_api_create_root_terminal_session,
    public_api_create_terminal_session, public_api_delete_root_terminal_session,
    public_api_delete_terminal_session, public_api_get_project, public_api_get_project_diff,
    public_api_get_project_terminal, public_api_get_root_terminal, public_api_list_projects,
    public_api_list_root_terminal_sessions, public_api_list_terminal_sessions, public_api_login,
    public_api_patch_project_diff, public_api_post_project_terminal, public_api_post_root_terminal,
    public_api_session, public_root_terminal_ws, public_terminal_ws,
};
pub(super) use serve::public_entry;

#[cfg(test)]
pub(super) use models::public_project_detail;
