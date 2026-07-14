use std::{collections::HashSet, path::PathBuf};

use tracing::warn;

use crate::{state::AppState, storage::DiscoveredWorktree};

use super::command::run_git_command;

pub(in crate::server) async fn discover_worktrees(state: &AppState) {
    let roots = match state.catalog().list_worktree_roots().await {
        Ok(roots) => roots,
        Err(error) => {
            warn!(%error, "worktree discovery roots could not be loaded");
            return;
        }
    };
    let mut reconciled = HashSet::new();

    for root in roots.into_iter().filter(|project| project.enabled) {
        let common_git_dir = match common_git_dir(&root.project_dir).await {
            Ok(path) => path,
            Err(_) => continue,
        };
        let common_key = common_git_dir.to_string_lossy().to_ascii_lowercase();
        if !reconciled.insert(common_key) {
            continue;
        }
        let worktrees = match list_git_worktrees(&root.project_dir).await {
            Ok(worktrees) => worktrees,
            Err(error) => {
                warn!(project = %root.name, %error, "Git worktrees could not be discovered");
                continue;
            }
        };
        if let Err(error) = state
            .catalog()
            .reconcile_worktrees(&common_git_dir, &root, &worktrees)
            .await
        {
            warn!(project = %root.name, %error, "Git worktrees could not be synchronized");
        }
    }
}

async fn common_git_dir(project_dir: &std::path::Path) -> Result<PathBuf, String> {
    let output = run_git_command(
        project_dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &[0],
    )
    .await?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Err("git rev-parse returned an empty common directory".to_string())
    } else {
        Ok(PathBuf::from(path))
    }
}

async fn list_git_worktrees(
    project_dir: &std::path::Path,
) -> Result<Vec<DiscoveredWorktree>, String> {
    let output = run_git_command(
        project_dir,
        &["worktree", "list", "--porcelain", "-z"],
        &[0],
    )
    .await?;
    Ok(parse_worktree_porcelain(&output.stdout))
}

fn parse_worktree_porcelain(output: &[u8]) -> Vec<DiscoveredWorktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<DiscoveredWorktree> = None;

    for field in output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
    {
        let field = String::from_utf8_lossy(field);
        if let Some(path) = field.strip_prefix("worktree ") {
            if let Some(worktree) = current.take() {
                worktrees.push(worktree);
            }
            current = Some(DiscoveredWorktree {
                worktree_dir: PathBuf::from(path),
                branch: None,
                head: String::new(),
            });
        } else if let Some(head) = field.strip_prefix("HEAD ") {
            if let Some(worktree) = current.as_mut() {
                worktree.head = head.to_string();
            }
        } else if let Some(branch) = field.strip_prefix("branch ") {
            if let Some(worktree) = current.as_mut() {
                worktree.branch = Some(
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_string(),
                );
            }
        }
    }
    if let Some(worktree) = current {
        worktrees.push(worktree);
    }
    worktrees
}

#[cfg(test)]
mod tests {
    use super::parse_worktree_porcelain;

    #[test]
    fn parses_linked_and_detached_worktrees() {
        let output = b"worktree C:/repo\0HEAD abc123\0branch refs/heads/master\0\0worktree C:/repo-fix\0HEAD def456\0detached\0\0";
        let parsed = parse_worktree_porcelain(output);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].branch.as_deref(), Some("master"));
        assert_eq!(parsed[1].branch, None);
        assert_eq!(parsed[1].head, "def456");
    }
}
