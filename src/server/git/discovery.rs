use std::{
    collections::HashSet,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use tokio::sync::Mutex;
use tracing::warn;

use crate::{config::ProjectConfig, state::AppState, storage::DiscoveredWorktree};

use super::command::run_git_command;

static DISCOVERY_RUNNING: AtomicBool = AtomicBool::new(false);
static DISCOVERY_LOCK: Mutex<()> = Mutex::const_new(());

pub(in crate::server) fn schedule_worktree_discovery(state: AppState) {
    if DISCOVERY_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    tokio::spawn(async move {
        let _guard = DiscoveryGuard;
        discover_worktrees(&state).await;
    });
}

struct DiscoveryGuard;

impl Drop for DiscoveryGuard {
    fn drop(&mut self) {
        DISCOVERY_RUNNING.store(false, Ordering::Release);
    }
}

pub(in crate::server) async fn discover_worktrees(state: &AppState) {
    let _discovery_guard = DISCOVERY_LOCK.lock().await;
    let roots = match state.catalog().list_worktree_roots().await {
        Ok(roots) => roots,
        Err(error) => {
            warn!(%error, "worktree discovery roots could not be loaded");
            return;
        }
    };
    let mut scans = tokio::task::JoinSet::new();
    for root in roots.into_iter().filter(|project| project.enabled) {
        scans.spawn(async move {
            let common_git_dir = match common_git_dir(&root.project_dir).await {
                Ok(path) => path,
                Err(_) => return (root, Ok(None)),
            };
            match list_git_worktrees(&root.project_dir).await {
                Ok(worktrees) => (root, Ok(Some((common_git_dir, worktrees)))),
                Err(error) => (root, Err(error)),
            }
        });
    }

    let mut reconciled = HashSet::new();
    while let Some(result) = scans.join_next().await {
        let Ok((root, scan)) = result else {
            continue;
        };
        let (common_git_dir, worktrees) = match scan {
            Ok(Some(scan)) => scan,
            Ok(None) => continue,
            Err(error) => {
                warn!(project = %root.name, %error, "Git worktrees could not be discovered");
                continue;
            }
        };
        let common_key = common_git_dir.to_string_lossy().to_ascii_lowercase();
        if !reconciled.insert(common_key) {
            continue;
        }
        if let Err(error) = state
            .catalog()
            .reconcile_worktrees(&common_git_dir, &root, &worktrees)
            .await
        {
            warn!(project = %root.name, %error, "Git worktrees could not be synchronized");
        }
    }
}

pub(in crate::server) async fn discover_worktree_project(
    state: &AppState,
    project_dir: &std::path::Path,
) -> Result<Option<ProjectConfig>, String> {
    let _discovery_guard = DISCOVERY_LOCK.lock().await;
    let common_git_dir = match common_git_dir(project_dir).await {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    let worktrees = list_git_worktrees(project_dir).await?;
    let Some(primary_worktree) = worktrees.first() else {
        return Ok(None);
    };
    let projects = state
        .catalog()
        .list_projects()
        .await
        .map_err(|error| error.to_string())?;
    let Some(root) = projects
        .into_iter()
        .find(|root| same_path(&root.project_dir, &primary_worktree.worktree_dir))
    else {
        return Ok(None);
    };

    state
        .catalog()
        .reconcile_worktrees(&common_git_dir, &root, &worktrees)
        .await
        .map_err(|error| error.to_string())?;

    let requested_dir = canonical_or_original(project_dir);
    let projects = state
        .catalog()
        .list_projects()
        .await
        .map_err(|error| error.to_string())?;
    let Some(project) = projects
        .into_iter()
        .find(|project| same_path(&project.project_dir, &requested_dir))
    else {
        return Ok(None);
    };
    state
        .catalog()
        .get_project(&project.name)
        .await
        .map_err(|error| error.to_string())
}

fn canonical_or_original(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    canonical_or_original(left)
        .to_string_lossy()
        .eq_ignore_ascii_case(&canonical_or_original(right).to_string_lossy())
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
