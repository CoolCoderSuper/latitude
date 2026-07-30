use std::path::{Path, PathBuf};

use tokio::{process::Command, sync::Semaphore, time::timeout};

use crate::workspace::{WorkspaceExecRequest, global_workspace_bridge};

use super::{super::constants::GIT_COMMAND_TIMEOUT, types::GitCommandOutput};

static GIT_COMMAND_CONCURRENCY: Semaphore = Semaphore::const_new(4);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) enum GitCommandExecution {
    Interactive,
    AutoRefresh,
}

impl GitCommandExecution {
    pub(in crate::server) fn uses_concurrency_pool(self) -> bool {
        matches!(self, Self::AutoRefresh)
    }
}

pub(super) async fn git_worktree_root(project_dir: &Path) -> Result<PathBuf, String> {
    let output = run_git_command(project_dir, &["rev-parse", "--show-toplevel"], &[0]).await?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if path.is_empty() {
        return Err("git rev-parse --show-toplevel returned an empty path".to_string());
    }

    Ok(PathBuf::from(path))
}

pub(super) async fn run_git_action_text(
    repo_dir: &Path,
    _title: &str,
    args: &[&str],
    success_codes: &[i32],
) -> Result<(), String> {
    run_git_command(repo_dir, args, success_codes)
        .await
        .map(|_| ())
}

pub(super) async fn run_git_action_text_owned(
    repo_dir: &Path,
    _title: &str,
    args: &[String],
    success_codes: &[i32],
) -> Result<(), String> {
    run_git_command_owned(repo_dir, args, success_codes)
        .await
        .map(|_| ())
}

pub(super) async fn run_git_command(
    project_dir: &Path,
    args: &[&str],
    success_codes: &[i32],
) -> Result<GitCommandOutput, String> {
    run_git_command_with_execution(
        project_dir,
        args,
        success_codes,
        GitCommandExecution::Interactive,
    )
    .await
}

pub(super) async fn run_git_command_with_execution(
    project_dir: &Path,
    args: &[&str],
    success_codes: &[i32],
    execution: GitCommandExecution,
) -> Result<GitCommandOutput, String> {
    let owned_args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    run_git_command_owned_with_execution(project_dir, &owned_args, success_codes, execution).await
}

pub(super) async fn run_git_command_owned(
    project_dir: &Path,
    args: &[String],
    success_codes: &[i32],
) -> Result<GitCommandOutput, String> {
    run_git_command_owned_with_execution(
        project_dir,
        args,
        success_codes,
        GitCommandExecution::Interactive,
    )
    .await
}

async fn run_git_command_owned_with_execution(
    project_dir: &Path,
    args: &[String],
    success_codes: &[i32],
    execution: GitCommandExecution,
) -> Result<GitCommandOutput, String> {
    let _permit = if execution.uses_concurrency_pool() {
        Some(
            GIT_COMMAND_CONCURRENCY
                .acquire()
                .await
                .map_err(|_| "Git command concurrency limiter closed".to_string())?,
        )
    } else {
        None
    };
    let mut command_args = vec![
        "-c".to_string(),
        format!("safe.directory={}", git_safe_directory(project_dir)),
    ];
    command_args.extend_from_slice(args);

    let (status_code, stdout, stderr) = if let Some(bridge) = global_workspace_bridge() {
        let output = bridge
            .execute(WorkspaceExecRequest::captured(
                "git",
                command_args,
                Some(project_dir.to_path_buf()),
                GIT_COMMAND_TIMEOUT,
                usize::MAX,
            ))
            .await
            .map_err(|error| {
                format!(
                    "Could not run {} in the signed-in user workspace: {error}",
                    git_command_label_owned(args)
                )
            })?;
        if output.timed_out {
            return Err(format!(
                "{} timed out after {} seconds",
                git_command_label_owned(args),
                GIT_COMMAND_TIMEOUT.as_secs()
            ));
        }
        if output.truncated {
            return Err(format!(
                "{} produced more workspace output than Latitude can safely transfer",
                git_command_label_owned(args)
            ));
        }
        (output.status_code, output.stdout, output.stderr)
    } else {
        let mut command = Command::new("git");
        command.args(&command_args).current_dir(project_dir);
        let output = match timeout(GIT_COMMAND_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return Err(format!(
                    "Could not run {}: {error}",
                    git_command_label_owned(args)
                ));
            }
            Err(_) => {
                return Err(format!(
                    "{} timed out after {} seconds",
                    git_command_label_owned(args),
                    GIT_COMMAND_TIMEOUT.as_secs()
                ));
            }
        };
        (output.status.code(), output.stdout, output.stderr)
    };

    if status_code.is_some_and(|code| success_codes.contains(&code)) {
        return Ok(GitCommandOutput { stdout, stderr });
    }

    let status = status_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "terminated".to_string());
    let mut message = format!(
        "{} exited with status {status}",
        git_command_label_owned(args)
    );
    let stderr = String::from_utf8_lossy(&stderr);
    let stdout = String::from_utf8_lossy(&stdout);
    if !stderr.trim().is_empty() {
        message.push_str("\n\n");
        message.push_str(stderr.trim());
    } else if !stdout.trim().is_empty() {
        message.push_str("\n\n");
        message.push_str(stdout.trim());
    }

    Err(message)
}

fn git_safe_directory(project_dir: &Path) -> String {
    project_dir.to_string_lossy().replace('\\', "/")
}

pub(super) fn parse_nul_separated_paths(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == b'\0')
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).replace('\\', "/"))
        .collect()
}

pub(super) fn git_command_label(args: &[&str]) -> String {
    let mut label = String::from("git");
    for arg in args {
        label.push(' ');
        label.push_str(arg);
    }
    label
}

fn git_command_label_owned(args: &[String]) -> String {
    let mut label = String::from("git");
    for arg in args {
        label.push(' ');
        label.push_str(arg);
    }
    label
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{GitCommandExecution, git_safe_directory};

    #[test]
    fn only_auto_refresh_commands_use_the_concurrency_pool() {
        assert!(!GitCommandExecution::Interactive.uses_concurrency_pool());
        assert!(GitCommandExecution::AutoRefresh.uses_concurrency_pool());
    }

    #[test]
    fn safe_directory_uses_git_friendly_path_separators() {
        assert_eq!(
            git_safe_directory(Path::new(r"C:\Users\Joseph\source\repos\latitude")),
            "C:/Users/Joseph/source/repos/latitude"
        );
    }
}
