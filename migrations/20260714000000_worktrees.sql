CREATE TABLE IF NOT EXISTS worktrees (
    project_name TEXT PRIMARY KEY,
    common_git_dir TEXT NOT NULL,
    worktree_dir TEXT NOT NULL UNIQUE,
    branch TEXT,
    head TEXT NOT NULL,
    discovered INTEGER NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (project_name) REFERENCES projects(name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS worktrees_common_git_dir_idx
    ON worktrees(common_git_dir);
