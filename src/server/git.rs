mod actions;
mod command;
mod diff;
mod discovery;
mod public;
mod submodules;
mod types;
pub(super) use submodules::{
    repository_summaries, repository_url, resolve_repository, submodule_paths,
};

#[cfg(test)]
pub(super) use actions::parse_git_action_form;
pub(super) use actions::{
    execute_git_action, handle_git_action_request, parse_public_git_action_payload,
};
pub(super) use command::GitCommandExecution;
pub(crate) use diff::file_baseline;
pub(super) use diff::{
    collect_project_diff, collect_project_file_diff, collect_project_git_commit,
    collect_project_git_history, collect_project_git_history_page, collect_project_git_status,
    collect_repository_view_diff,
};
#[cfg(test)]
pub(super) use diff::{parse_diff_file_sections, parse_porcelain_status};
pub(super) use discovery::{discover_worktree_project, discover_worktrees};
pub(super) use public::{
    PublicGitActionResponse, public_commit_response, public_diff_response, public_history_response,
};
pub(super) use types::{
    FileSectionKind, GitAction, GitCommitReport, GitFileChange, GitFileDiff, GitHistoryReport,
};
pub(crate) use types::{GitDiffReport, GitStatusSummary};
