use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug)]
pub(in crate::server) struct GitDiffReport {
    pub(in crate::server) repo_dir: PathBuf,
    pub(in crate::server) file_changes: Vec<GitFileChange>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct GitActionResponse {
    pub(in crate::server) ok: bool,
    pub(in crate::server) error: Option<String>,
    pub(in crate::server) workspace_html: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::server) enum GitAction {
    StageAll,
    StageFile { path: String },
    UnstageAll,
    UnstageFile { path: String },
    DiscardAll,
    DiscardFile { path: String },
    Commit { message: String },
    Push,
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
