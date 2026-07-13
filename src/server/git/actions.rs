use std::path::Path;

use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde::Deserialize;

use super::{
    super::{
        constants::MAX_DIFF_ACTION_PAYLOAD_BYTES,
        page::{content_type_media_type, is_json_media_type},
    },
    command::{
        git_worktree_root, parse_nul_separated_paths, run_git_action_text,
        run_git_action_text_owned, run_git_command, run_git_command_owned,
    },
    types::GitAction,
};

#[derive(Debug, Deserialize)]
struct PublicGitActionPayload {
    action: String,
    path: Option<String>,
    message: Option<String>,
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
            "discard_all" => Ok(GitAction::DiscardAll),
            "discard_file" => Ok(GitAction::DiscardFile {
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
            "pull" => Ok(GitAction::Pull),
            "push" => Ok(GitAction::Push),
            action if !action.is_empty() => Err(format!("unknown git action '{action}'")),
            _ => Err("git action is required".to_string()),
        }
    }
}

pub(in crate::server) async fn handle_git_action_request(
    req: Request<Body>,
    project_dir: &Path,
) -> Result<GitAction, String> {
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_DIFF_ACTION_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => return Err(format!("action payload could not be read: {error}")),
    };

    let action = parse_git_action_form(&body)?;
    execute_git_action(project_dir, action.clone()).await?;
    Ok(action)
}

pub(in crate::server) async fn execute_git_action(
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
        GitAction::DiscardAll => discard_unstaged_all(&repo_dir).await,
        GitAction::DiscardFile { path } => discard_unstaged_file(&repo_dir, path).await,
        GitAction::Commit { message } => {
            run_git_action_text_owned(
                &repo_dir,
                "Commit staged",
                &["commit".to_string(), "-m".to_string(), message],
                &[0],
            )
            .await
        }
        GitAction::Pull => {
            run_git_action_text(&repo_dir, "Pull", &["pull", "--ff-only"], &[0]).await
        }
        GitAction::Push => run_git_action_text(&repo_dir, "Push", &["push"], &[0]).await,
    }
}

async fn discard_unstaged_file(repo_dir: &Path, path: String) -> Result<(), String> {
    if git_path_is_untracked(repo_dir, &path).await? {
        return run_git_action_text_owned(
            repo_dir,
            "Discard untracked file",
            &[
                "clean".to_string(),
                "-fd".to_string(),
                "--".to_string(),
                path,
            ],
            &[0],
        )
        .await;
    }

    run_git_action_text_owned(
        repo_dir,
        "Discard file changes",
        &[
            "restore".to_string(),
            "--worktree".to_string(),
            "--".to_string(),
            path,
        ],
        &[0],
    )
    .await
}

async fn discard_unstaged_all(repo_dir: &Path) -> Result<(), String> {
    if !git_tracked_unstaged_paths(repo_dir).await?.is_empty() {
        run_git_action_text(
            repo_dir,
            "Discard tracked changes",
            &["restore", "--worktree", "--", "."],
            &[0],
        )
        .await?;
    }

    run_git_action_text(
        repo_dir,
        "Discard untracked files",
        &["clean", "-fd", "--", "."],
        &[0],
    )
    .await
}

async fn git_path_is_untracked(repo_dir: &Path, path: &str) -> Result<bool, String> {
    let output = run_git_command_owned(
        repo_dir,
        &[
            "ls-files".to_string(),
            "--others".to_string(),
            "--exclude-standard".to_string(),
            "-z".to_string(),
            "--".to_string(),
            path.to_string(),
        ],
        &[0],
    )
    .await?;

    Ok(!parse_nul_separated_paths(&output.stdout).is_empty())
}

async fn git_tracked_unstaged_paths(repo_dir: &Path) -> Result<Vec<String>, String> {
    let output = run_git_command(repo_dir, &["diff", "--name-only", "-z"], &[0]).await?;
    Ok(parse_nul_separated_paths(&output.stdout))
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

pub(in crate::server) fn parse_git_action_form(body: &[u8]) -> Result<GitAction, String> {
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
        Some("discard_all") => Ok(GitAction::DiscardAll),
        Some("discard_file") => {
            let path = clean_git_form_path(path)?;
            Ok(GitAction::DiscardFile { path })
        }
        Some("commit") => {
            let message = message.unwrap_or_default().trim().to_string();
            if message.is_empty() {
                Err("commit message is required".to_string())
            } else {
                Ok(GitAction::Commit { message })
            }
        }
        Some("pull") => Ok(GitAction::Pull),
        Some("push") => Ok(GitAction::Push),
        Some(action) if !action.is_empty() => Err(format!("unknown git action '{action}'")),
        _ => Err("git action is required".to_string()),
    }
}

pub(in crate::server) fn parse_public_git_action_payload(
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
