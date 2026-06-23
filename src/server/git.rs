use std::path::{Path, PathBuf};

use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde::{Deserialize, Serialize};
use tokio::{fs, process::Command, time::timeout};

use super::{
    constants::{GIT_COMMAND_TIMEOUT, MAX_DIFF_ACTION_PAYLOAD_BYTES},
    page::{content_type_media_type, is_json_media_type},
    paths::display_path,
};

#[derive(Debug, Serialize)]
pub(super) struct PublicGitDiffResponse {
    pub(super) repo_dir: String,
    pub(super) unstaged_count: usize,
    pub(super) staged_count: usize,
    pub(super) file_changes: Vec<GitFileChange>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PublicGitActionPayload {
    pub(super) action: String,
    pub(super) path: Option<String>,
    pub(super) message: Option<String>,
}

impl PublicGitActionPayload {
    fn into_git_action(self) -> Result<GitAction, String> {
        match self.action.trim() {
            "stage_all" => Ok(GitAction::StageAll),
            "stage_file" => Ok(GitAction::StageFile {
                path: clean_git_form_path(self.path)?,
            }),
            "unstage_all" => Ok(GitAction::UnstageAll),
            "unstage_file" => Ok(GitAction::UnstageFile {
                path: clean_git_form_path(self.path)?,
            }),
            "commit" => {
                let message = self.message.unwrap_or_default().trim().to_string();
                if message.is_empty() {
                    Err("commit message is required".to_string())
                } else {
                    Ok(GitAction::Commit { message })
                }
            }
            "push" => Ok(GitAction::Push),
            action if !action.is_empty() => Err(format!("unknown git action '{action}'")),
            _ => Err("git action is required".to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct PublicGitActionResponse {
    pub(super) ok: bool,
    pub(super) error: Option<String>,
    pub(super) diff: PublicGitDiffResponse,
}

#[derive(Debug)]
pub(super) struct GitDiffReport {
    pub(super) repo_dir: PathBuf,
    pub(super) file_changes: Vec<GitFileChange>,
}

#[derive(Debug, Serialize)]
pub(super) struct GitActionResponse {
    pub(super) ok: bool,
    pub(super) error: Option<String>,
    pub(super) workspace_html: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum GitAction {
    StageAll,
    StageFile { path: String },
    UnstageAll,
    UnstageFile { path: String },
    Commit { message: String },
    Push,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(super) struct GitFileChange {
    pub(super) path: String,
    pub(super) original_path: Option<String>,
    pub(super) index_status: char,
    pub(super) worktree_status: char,
    pub(super) diffs: Vec<GitFileDiff>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(super) struct GitFileDiff {
    pub(super) label: String,
    pub(super) command: String,
    pub(super) path: String,
    pub(super) content: String,
}

impl GitFileChange {
    pub(super) fn status_label(&self) -> String {
        format!("{}{}", self.index_status, self.worktree_status)
    }

    pub(super) fn can_stage(&self) -> bool {
        self.index_status == '?' || self.worktree_status != ' '
    }

    pub(super) fn can_unstage(&self) -> bool {
        self.index_status != ' ' && self.index_status != '?' && self.index_status != '!'
    }
}

#[derive(Clone, Copy)]
pub(super) enum FileSectionKind {
    Unstaged,
    Staged,
}

impl FileSectionKind {
    pub(super) fn includes(self, change: &GitFileChange) -> bool {
        match self {
            Self::Unstaged => change.can_stage(),
            Self::Staged => change.can_unstage(),
        }
    }

    pub(super) fn includes_diff(self, diff: &GitFileDiff) -> bool {
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

pub(super) async fn handle_git_action_request(
    req: Request<Body>,
    project_dir: &Path,
) -> Result<(), String> {
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_DIFF_ACTION_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => return Err(format!("action payload could not be read: {error}")),
    };

    match parse_git_action_form(&body) {
        Ok(action) => execute_git_action(project_dir, action).await,
        Err(error) => Err(error),
    }
}

pub(super) async fn execute_git_action(
    project_dir: &Path,
    action: GitAction,
) -> Result<(), String> {
    let repo_dir = match git_worktree_root(project_dir).await {
        Ok(path) => path,
        Err(error) => return Err(error),
    };

    match action {
        GitAction::StageAll => {
            run_git_action_text(&repo_dir, "Stage all", &["add", "--all"], &[0]).await
        }
        GitAction::StageFile { path } => {
            run_git_action_text_owned(
                &repo_dir,
                "Stage file",
                &["add".to_string(), "--".to_string(), path],
                &[0],
            )
            .await
        }
        GitAction::UnstageAll => unstage_all(&repo_dir).await,
        GitAction::UnstageFile { path } => unstage_file(&repo_dir, path).await,
        GitAction::Commit { message } => {
            run_git_action_text_owned(
                &repo_dir,
                "Commit staged",
                &["commit".to_string(), "-m".to_string(), message],
                &[0],
            )
            .await
        }
        GitAction::Push => run_git_action_text(&repo_dir, "Push", &["push"], &[0]).await,
    }
}

async fn unstage_file(repo_dir: &Path, path: String) -> Result<(), String> {
    let has_head = run_git_command(repo_dir, &["rev-parse", "--verify", "HEAD"], &[0])
        .await
        .is_ok();

    if has_head {
        run_git_action_text_owned(
            repo_dir,
            "Unstage file",
            &[
                "reset".to_string(),
                "-q".to_string(),
                "HEAD".to_string(),
                "--".to_string(),
                path,
            ],
            &[0],
        )
        .await
    } else {
        run_git_action_text_owned(
            repo_dir,
            "Unstage file",
            &[
                "rm".to_string(),
                "--cached".to_string(),
                "-r".to_string(),
                "--ignore-unmatch".to_string(),
                "--".to_string(),
                path,
            ],
            &[0],
        )
        .await
    }
}

async fn unstage_all(repo_dir: &Path) -> Result<(), String> {
    let has_head = run_git_command(repo_dir, &["rev-parse", "--verify", "HEAD"], &[0])
        .await
        .is_ok();

    if has_head {
        run_git_action_text(repo_dir, "Unstage all", &["reset", "-q", "HEAD"], &[0]).await
    } else {
        run_git_action_text(
            repo_dir,
            "Unstage all",
            &["rm", "--cached", "-r", "--ignore-unmatch", "."],
            &[0],
        )
        .await
    }
}

async fn git_worktree_root(project_dir: &Path) -> Result<PathBuf, String> {
    let output = run_git_command(project_dir, &["rev-parse", "--show-toplevel"], &[0]).await?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if path.is_empty() {
        return Err("git rev-parse --show-toplevel returned an empty path".to_string());
    }

    Ok(PathBuf::from(path))
}

async fn run_git_action_text(
    repo_dir: &Path,
    _title: &str,
    args: &[&str],
    success_codes: &[i32],
) -> Result<(), String> {
    run_git_command(repo_dir, args, success_codes)
        .await
        .map(|_| ())
}

async fn run_git_action_text_owned(
    repo_dir: &Path,
    _title: &str,
    args: &[String],
    success_codes: &[i32],
) -> Result<(), String> {
    run_git_command_owned(repo_dir, args, success_codes)
        .await
        .map(|_| ())
}

pub(super) async fn collect_project_diff(project_dir: &Path) -> GitDiffReport {
    let fallback_dir = fs::canonicalize(project_dir)
        .await
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let repo_dir = git_worktree_root(project_dir)
        .await
        .unwrap_or_else(|_| fallback_dir.clone());
    let status = collect_git_text(
        &repo_dir,
        &["status", "--short", "--branch", "--untracked-files=all"],
        &[0],
    )
    .await;

    if status.output.is_err() {
        return GitDiffReport {
            repo_dir,
            file_changes: Vec::new(),
        };
    }

    let mut file_changes = collect_git_file_changes(&repo_dir)
        .await
        .unwrap_or_default();
    let unstaged_diff =
        collect_git_text(&repo_dir, &["diff", "--no-ext-diff", "--color=never"], &[0]).await;
    let staged_diff = collect_git_text(
        &repo_dir,
        &["diff", "--cached", "--no-ext-diff", "--color=never"],
        &[0],
    )
    .await;
    let untracked_diff = collect_untracked_diff(&repo_dir).await;
    attach_file_diffs(
        &mut file_changes,
        "Unstaged",
        &unstaged_diff,
        section_output(&unstaged_diff),
    );
    attach_file_diffs(
        &mut file_changes,
        "Staged",
        &staged_diff,
        section_output(&staged_diff),
    );
    attach_file_diffs(
        &mut file_changes,
        "Untracked",
        &untracked_diff,
        section_output(&untracked_diff),
    );

    GitDiffReport {
        repo_dir,
        file_changes,
    }
}

async fn collect_git_file_changes(repo_dir: &Path) -> Result<Vec<GitFileChange>, String> {
    let output = run_git_command(
        repo_dir,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        &[0],
    )
    .await?;

    Ok(parse_porcelain_status(&output.stdout))
}

fn attach_file_diffs(
    changes: &mut [GitFileChange],
    label: &str,
    section: &GitSection,
    content: Option<&str>,
) {
    let Some(content) = content else {
        return;
    };

    for diff in parse_diff_file_sections(label, &section.command, content) {
        let Some(change) = changes.iter_mut().find(|change| {
            change.path == diff.path || change.original_path.as_ref() == Some(&diff.path)
        }) else {
            continue;
        };

        change.diffs.push(diff);
    }
}

fn section_output(section: &GitSection) -> Option<&str> {
    section.output.as_ref().ok().map(String::as_str)
}

async fn collect_git_text(project_dir: &Path, args: &[&str], success_codes: &[i32]) -> GitSection {
    let command = git_command_label(args);
    let output = run_git_command(project_dir, args, success_codes)
        .await
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string());

    GitSection { command, output }
}

async fn collect_untracked_diff(project_dir: &Path) -> GitSection {
    let command = git_command_label(&[
        "diff",
        "--no-index",
        "--color=never",
        "--",
        "/dev/null",
        "<untracked-file>",
    ]);
    let files = match run_git_command(
        project_dir,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        &[0],
    )
    .await
    {
        Ok(output) => parse_nul_separated_paths(&output.stdout),
        Err(error) => {
            return GitSection {
                command,
                output: Err(error),
            };
        }
    };

    if files.is_empty() {
        return GitSection {
            command,
            output: Ok(String::new()),
        };
    }

    let mut combined = String::new();
    for file in files {
        let output = run_git_command(
            project_dir,
            &[
                "diff",
                "--no-index",
                "--color=never",
                "--",
                "/dev/null",
                file.as_str(),
            ],
            &[0, 1],
        )
        .await;

        match output {
            Ok(output) => {
                combined.push_str(&String::from_utf8_lossy(&output.stdout));
                if !output.stderr.is_empty() {
                    if !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                    combined.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                if !combined.ends_with('\n') {
                    combined.push('\n');
                }
            }
            Err(error) => {
                if !combined.ends_with('\n') {
                    combined.push('\n');
                }
                combined.push_str("diff --git a/");
                combined.push_str(&file);
                combined.push_str(" b/");
                combined.push_str(&file);
                combined.push('\n');
                combined.push_str(&error);
                combined.push('\n');
            }
        }
    }

    GitSection {
        command,
        output: Ok(combined),
    }
}

async fn run_git_command(
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

async fn run_git_command_owned(
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

pub(super) fn parse_git_action_form(body: &[u8]) -> Result<GitAction, String> {
    let mut action = None;
    let mut message = None;
    let mut path = None;

    for (key, value) in url::form_urlencoded::parse(body) {
        match key.as_ref() {
            "action" => action = Some(value.into_owned()),
            "message" => message = Some(value.into_owned()),
            "path" => path = Some(value.into_owned()),
            _ => {}
        }
    }

    match action.as_deref().map(str::trim) {
        Some("stage_all") => Ok(GitAction::StageAll),
        Some("stage_file") => {
            let path = clean_git_form_path(path)?;
            Ok(GitAction::StageFile { path })
        }
        Some("unstage_all") => Ok(GitAction::UnstageAll),
        Some("unstage_file") => {
            let path = clean_git_form_path(path)?;
            Ok(GitAction::UnstageFile { path })
        }
        Some("commit") => {
            let message = message.unwrap_or_default().trim().to_string();
            if message.is_empty() {
                Err("commit message is required".to_string())
            } else {
                Ok(GitAction::Commit { message })
            }
        }
        Some("push") => Ok(GitAction::Push),
        Some(action) if !action.is_empty() => Err(format!("unknown git action '{action}'")),
        _ => Err("git action is required".to_string()),
    }
}

pub(super) fn parse_public_git_action_payload(
    content_type: Option<&str>,
    body: &[u8],
) -> Result<GitAction, String> {
    if content_type_media_type(content_type)
        .as_deref()
        .is_some_and(is_json_media_type)
    {
        let payload: PublicGitActionPayload = serde_json::from_slice(body)
            .map_err(|error| format!("git action JSON payload is invalid: {error}"))?;
        return payload.into_git_action();
    }

    parse_git_action_form(body)
}

fn clean_git_form_path(path: Option<String>) -> Result<String, String> {
    let path = path.unwrap_or_default().trim().replace('\\', "/");
    if path.is_empty() {
        Err("path is required".to_string())
    } else {
        Ok(path)
    }
}

pub(super) fn parse_porcelain_status(bytes: &[u8]) -> Vec<GitFileChange> {
    let entries = parse_nul_separated_paths(bytes);
    let mut changes = Vec::new();
    let mut index = 0;

    while index < entries.len() {
        let entry = &entries[index];
        index += 1;

        if entry.len() < 4 {
            continue;
        }

        let mut chars = entry.chars();
        let index_status = chars.next().unwrap_or(' ');
        let worktree_status = chars.next().unwrap_or(' ');
        if chars.next() != Some(' ') {
            continue;
        }

        let path = chars.as_str().to_string();
        if path.is_empty() {
            continue;
        }

        let original_path = if matches!(index_status, 'R' | 'C') && index < entries.len() {
            let original = entries[index].clone();
            index += 1;
            Some(original)
        } else {
            None
        };

        changes.push(GitFileChange {
            path,
            original_path,
            index_status,
            worktree_status,
            diffs: Vec::new(),
        });
    }

    changes
}

pub(super) fn parse_diff_file_sections(
    label: &str,
    command: &str,
    content: &str,
) -> Vec<GitFileDiff> {
    let mut sections = Vec::new();
    let mut current_path = None::<String>;
    let mut current_content = String::new();

    for line in content.lines() {
        if line.starts_with("diff --git ") {
            if let Some(path) = current_path.take() {
                sections.push(GitFileDiff {
                    label: label.to_string(),
                    command: command.to_string(),
                    path,
                    content: current_content.trim_end().to_string(),
                });
                current_content.clear();
            }

            current_path = diff_git_line_path(line);
        }

        if current_path.is_some() {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if let Some(path) = current_path {
        sections.push(GitFileDiff {
            label: label.to_string(),
            command: command.to_string(),
            path,
            content: current_content.trim_end().to_string(),
        });
    }

    sections
}

fn diff_git_line_path(line: &str) -> Option<String> {
    let rest = line.strip_prefix("diff --git ")?;
    let (_, after_b) = rest.split_once(" b/")?;
    Some(after_b.trim_matches('"').to_string())
}

fn parse_nul_separated_paths(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == b'\0')
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).replace('\\', "/"))
        .collect()
}

fn git_command_label(args: &[&str]) -> String {
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

pub(super) fn public_diff_response(report: GitDiffReport) -> PublicGitDiffResponse {
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

    PublicGitDiffResponse {
        repo_dir: display_path(&report.repo_dir),
        unstaged_count,
        staged_count,
        file_changes: report.file_changes,
    }
}
