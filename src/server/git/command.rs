use std::path::{Path, PathBuf};

use tokio::{process::Command, time::timeout};

use super::{super::constants::GIT_COMMAND_TIMEOUT, types::GitCommandOutput};

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
    let owned_args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    run_git_command_owned(project_dir, &owned_args, success_codes).await
}

pub(super) async fn run_git_command_owned(
    project_dir: &Path,
    args: &[String],
    success_codes: &[i32],
) -> Result<GitCommandOutput, String> {
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
