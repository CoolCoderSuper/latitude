use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug)]
pub(in crate::server) struct GitDiffReport {
    pub(in crate::server) repo_dir: PathBuf,
    pub(in crate::server) status: GitStatusSummary,
    pub(in crate::server) file_changes: Vec<GitFileChange>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub(in crate::server) struct GitStatusSummary {
    pub(in crate::server) dirty: bool,
    pub(in crate::server) additions: usize,
    pub(in crate::server) deletions: usize,
    pub(in crate::server) ahead: usize,
    pub(in crate::server) behind: usize,
}

impl GitStatusSummary {
    pub(in crate::server) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(in crate::server) fn has_status(&self) -> bool {
        self.is_dirty() || self.ahead > 0 || self.behind > 0
    }

    pub(in crate::server) fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.is_dirty() {
            if self.additions > 0 {
                parts.push(format!("+{}", self.additions));
            }
            if self.deletions > 0 {
                parts.push(format!("-{}", self.deletions));
            }
            if self.additions == 0 && self.deletions == 0 {
                parts.push("changes".to_string());
            }
        }
        if self.behind > 0 {
            parts.push(format!("↓{}", self.behind));
        }
        if self.ahead > 0 {
            parts.push(format!("↑{}", self.ahead));
        }
        parts.join(" ")
    }

    pub(in crate::server) fn accessible_label(&self) -> String {
        let mut parts = Vec::new();
        if self.is_dirty() {
            if self.additions > 0 {
                parts.push(format!("{} additions", self.additions));
            }
            if self.deletions > 0 {
                parts.push(format!("{} deletions", self.deletions));
            }
            if self.additions == 0 && self.deletions == 0 {
                parts.push("working tree changes".to_string());
            }
        }
        if self.behind > 0 {
            parts.push(commit_sync_label(self.behind, "pull"));
        }
        if self.ahead > 0 {
            parts.push(commit_sync_label(self.ahead, "push"));
        }
        parts.join(", ")
    }
}

fn commit_sync_label(count: usize, action: &str) -> String {
    let noun = if count == 1 { "commit" } else { "commits" };
    format!("{count} {noun} to {action}")
}

#[derive(Debug)]
pub(in crate::server) struct GitHistoryReport {
    pub(in crate::server) repo_dir: PathBuf,
    pub(in crate::server) commits: Vec<GitCommit>,
}

#[derive(Debug)]
pub(in crate::server) struct GitCommitReport {
    pub(in crate::server) repo_dir: PathBuf,
    pub(in crate::server) commit: GitCommit,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::server) struct GitCommit {
    pub(in crate::server) hash: String,
    pub(in crate::server) short_hash: String,
    pub(in crate::server) author: String,
    pub(in crate::server) authored_at: String,
    pub(in crate::server) subject: String,
    pub(in crate::server) diff: String,
    pub(in crate::server) files: Vec<GitFileDiff>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::server) enum GitAction {
    StageAll,
    StageFile { path: String },
    StageFiles { paths: Vec<String> },
    UnstageAll,
    UnstageFile { path: String },
    UnstageFiles { paths: Vec<String> },
    DiscardAll,
    DiscardFile { path: String },
    Commit { message: String },
    Fetch,
    Pull,
    Push,
}

impl GitAction {
    pub(in crate::server) fn affected_path(&self) -> Option<&str> {
        match self {
            Self::StageFile { path } | Self::UnstageFile { path } | Self::DiscardFile { path } => {
                Some(path)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(in crate::server) struct GitFileChange {
    pub(in crate::server) path: String,
    pub(in crate::server) original_path: Option<String>,
    pub(in crate::server) index_status: char,
    pub(in crate::server) worktree_status: char,
    pub(in crate::server) diffs: Vec<GitFileDiff>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(in crate::server) struct GitFileDiff {
    pub(in crate::server) label: String,
    pub(in crate::server) command: String,
    pub(in crate::server) path: String,
    pub(in crate::server) content: String,
}

impl GitFileChange {
    pub(in crate::server) fn status_label(&self) -> String {
        format!("{}{}", self.index_status, self.worktree_status)
    }

    pub(in crate::server) fn can_stage(&self) -> bool {
        self.index_status == '?' || self.worktree_status != ' '
    }

    pub(in crate::server) fn can_unstage(&self) -> bool {
        self.index_status != ' ' && self.index_status != '?' && self.index_status != '!'
    }

    pub(in crate::server) fn can_open_in_editor(&self) -> bool {
        self.index_status != 'D' && self.worktree_status != 'D'
    }
}

#[derive(Clone, Copy)]
pub(in crate::server) enum FileSectionKind {
    Unstaged,
    Staged,
}

impl FileSectionKind {
    pub(in crate::server) fn includes(self, change: &GitFileChange) -> bool {
        match self {
            Self::Unstaged => change.can_stage(),
            Self::Staged => change.can_unstage(),
        }
    }

    pub(in crate::server) fn includes_diff(self, diff: &GitFileDiff) -> bool {
        match self {
            Self::Unstaged => diff.label == "Unstaged" || diff.label == "Untracked",
            Self::Staged => diff.label == "Staged",
        }
    }

    pub(in crate::server) fn data_key(self) -> &'static str {
        match self {
            Self::Unstaged => "unstaged",
            Self::Staged => "staged",
        }
    }
}

#[derive(Debug)]
pub(super) struct GitSection {
    pub(super) command: String,
    pub(super) output: Result<String, String>,
}

#[derive(Debug)]
pub(super) struct GitCommandOutput {
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
}
