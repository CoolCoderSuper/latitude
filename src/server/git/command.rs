use std::path::{Path, PathBuf};

use tokio::{process::Command, sync::Semaphore, time::timeout};

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
    let mut command = Command::new("git");
    command.args(args).current_dir(project_dir);

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

    if output
        .status
        .code()
        .is_some_and(|code| success_codes.contains(&code))
    {
        return Ok(GitCommandOutput {
            stdout: output.stdout,
            stderr: output.stderr,
        });
    }

    let status = output
        .status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "terminated".to_string());
    let mut message = format!(
        "{} exited with status {status}",
        git_command_label_owned(args)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stderr.trim().is_empty() {
        message.push_str("\n\n");
        message.push_str(stderr.trim());
    } else if !stdout.trim().is_empty() {
        message.push_str("\n\n");
        message.push_str(stdout.trim());
    }

    Err(message)
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
    use super::GitCommandExecution;

    #[test]
    fn only_auto_refresh_commands_use_the_concurrency_pool() {
        assert!(!GitCommandExecution::Interactive.uses_concurrency_pool());
        assert!(GitCommandExecution::AutoRefresh.uses_concurrency_pool());
    }
}
