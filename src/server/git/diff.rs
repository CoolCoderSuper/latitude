use std::path::Path;

use tokio::fs;

use super::{
    command::{git_command_label, git_worktree_root, parse_nul_separated_paths, run_git_command},
    types::{GitDiffReport, GitFileChange, GitFileDiff, GitSection},
};

pub(in crate::server) async fn collect_project_diff(project_dir: &Path) -> GitDiffReport {
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

pub(in crate::server) fn parse_porcelain_status(bytes: &[u8]) -> Vec<GitFileChange> {
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

pub(in crate::server) fn parse_diff_file_sections(
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
