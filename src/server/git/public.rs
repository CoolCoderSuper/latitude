use serde::Serialize;

use super::{
    super::{
        paths::display_path,
        render::{HighlightedDiffLine, highlight_diff_lines},
    },
    types::{FileSectionKind, GitDiffReport, GitFileChange, GitFileDiff},
};

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicGitDiffResponse {
    pub(in crate::server) repo_dir: String,
    pub(in crate::server) unstaged_count: usize,
    pub(in crate::server) staged_count: usize,
    pub(in crate::server) file_changes: Vec<PublicGitFileChange>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicGitFileChange {
    pub(in crate::server) path: String,
    pub(in crate::server) original_path: Option<String>,
    pub(in crate::server) index_status: char,
    pub(in crate::server) worktree_status: char,
    pub(in crate::server) diffs: Vec<PublicGitFileDiff>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicGitFileDiff {
    pub(in crate::server) label: String,
    pub(in crate::server) command: String,
    pub(in crate::server) path: String,
    pub(in crate::server) content: String,
    pub(in crate::server) lines: Vec<HighlightedDiffLine>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicGitActionResponse {
    pub(in crate::server) ok: bool,
    pub(in crate::server) error: Option<String>,
    pub(in crate::server) diff: PublicGitDiffResponse,
}

pub(in crate::server) fn public_diff_response(report: GitDiffReport) -> PublicGitDiffResponse {
    let unstaged_count = report
        .file_changes
        .iter()
        .filter(|change| FileSectionKind::Unstaged.includes(change))
        .count();
    let staged_count = report
        .file_changes
        .iter()
        .filter(|change| FileSectionKind::Staged.includes(change))
        .count();
    let file_changes = report
        .file_changes
        .into_iter()
        .map(public_git_file_change)
        .collect();

    PublicGitDiffResponse {
        repo_dir: display_path(&report.repo_dir),
        unstaged_count,
        staged_count,
        file_changes,
    }
}

fn public_git_file_change(change: GitFileChange) -> PublicGitFileChange {
    PublicGitFileChange {
        path: change.path,
        original_path: change.original_path,
        index_status: change.index_status,
        worktree_status: change.worktree_status,
        diffs: change.diffs.into_iter().map(public_git_file_diff).collect(),
    }
}

fn public_git_file_diff(diff: GitFileDiff) -> PublicGitFileDiff {
    let lines = highlight_diff_lines(&diff.content, &diff.path);

    PublicGitFileDiff {
        label: diff.label,
        command: diff.command,
        path: diff.path,
        content: diff.content,
        lines,
    }
}
