use serde::Serialize;

use super::{
    super::{
        paths::display_path,
        render::{HighlightedDiffLine, highlight_diff_lines},
    },
    types::{
        FileSectionKind, GitCommitReport, GitDiffReport, GitFileChange, GitFileDiff,
        GitHistoryReport,
    },
};

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicGitDiffResponse {
    pub(in crate::server) repo_dir: String,
    pub(in crate::server) unstaged_count: usize,
    pub(in crate::server) staged_count: usize,
    pub(in crate::server) additions: usize,
    pub(in crate::server) deletions: usize,
    pub(in crate::server) ahead: usize,
    pub(in crate::server) behind: usize,
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

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicGitHistoryResponse {
    pub(in crate::server) repo_dir: String,
    pub(in crate::server) commits: Vec<PublicGitCommitSummary>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicGitCommitSummary {
    pub(in crate::server) hash: String,
    pub(in crate::server) short_hash: String,
    pub(in crate::server) author: String,
    pub(in crate::server) authored_at: String,
    pub(in crate::server) subject: String,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicGitCommitResponse {
    pub(in crate::server) repo_dir: String,
    pub(in crate::server) hash: String,
    pub(in crate::server) short_hash: String,
    pub(in crate::server) author: String,
    pub(in crate::server) authored_at: String,
    pub(in crate::server) subject: String,
    pub(in crate::server) additions: usize,
    pub(in crate::server) deletions: usize,
    pub(in crate::server) files: Vec<PublicGitFileDiff>,
}

pub(in crate::server) fn public_history_response(
    report: GitHistoryReport,
) -> PublicGitHistoryResponse {
    PublicGitHistoryResponse {
        repo_dir: display_path(&report.repo_dir),
        commits: report
            .commits
            .into_iter()
            .map(|commit| PublicGitCommitSummary {
                hash: commit.hash,
                short_hash: commit.short_hash,
                author: commit.author,
                authored_at: commit.authored_at,
                subject: commit.subject,
            })
            .collect(),
    }
}

pub(in crate::server) fn public_commit_response(
    report: GitCommitReport,
) -> PublicGitCommitResponse {
    let (additions, deletions) = report.commit.files.iter().fold((0, 0), |totals, file| {
        let additions = file
            .content
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .count();
        let deletions = file
            .content
            .lines()
            .filter(|line| line.starts_with('-') && !line.starts_with("---"))
            .count();
        (totals.0 + additions, totals.1 + deletions)
    });
    PublicGitCommitResponse {
        repo_dir: display_path(&report.repo_dir),
        hash: report.commit.hash,
        short_hash: report.commit.short_hash,
        author: report.commit.author,
        authored_at: report.commit.authored_at,
        subject: report.commit.subject,
        additions,
        deletions,
        files: report
            .commit
            .files
            .into_iter()
            .map(public_git_file_diff)
            .collect(),
    }
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
        additions: report.status.additions,
        deletions: report.status.deletions,
        ahead: report.status.ahead,
        behind: report.status.behind,
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
