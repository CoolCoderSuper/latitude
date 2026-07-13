use std::path::Path;

use tokio::fs;

use super::{
    command::{
        git_command_label, git_worktree_root, parse_nul_separated_paths, run_git_command,
        run_git_command_owned,
    },
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

pub(in crate::server) async fn collect_project_file_diff(
    project_dir: &Path,
    path: &str,
) -> GitDiffReport {
    let fallback_dir = fs::canonicalize(project_dir)
        .await
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let repo_dir = git_worktree_root(project_dir)
        .await
        .unwrap_or_else(|_| fallback_dir.clone());
    let status_args = vec![
        "status".to_string(),
        "--porcelain=v1".to_string(),
        "-z".to_string(),
        "--untracked-files=all".to_string(),
        "--".to_string(),
        path.to_string(),
    ];
    let mut file_changes = run_git_command_owned(&repo_dir, &status_args, &[0])
        .await
        .map(|output| parse_porcelain_status(&output.stdout))
        .unwrap_or_default();

    if file_changes.is_empty() {
        return GitDiffReport {
            repo_dir,
            file_changes,
        };
    }

    let unstaged_args = vec![
        "diff".to_string(),
        "--no-ext-diff".to_string(),
        "--color=never".to_string(),
        "--".to_string(),
        path.to_string(),
    ];
    let staged_args = vec![
        "diff".to_string(),
        "--cached".to_string(),
        "--no-ext-diff".to_string(),
        "--color=never".to_string(),
        "--".to_string(),
        path.to_string(),
    ];
    let unstaged_diff = collect_git_text_owned(&repo_dir, &unstaged_args, &[0]).await;
    let staged_diff = collect_git_text_owned(&repo_dir, &staged_args, &[0]).await;
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

    if file_changes.iter().any(|change| change.index_status == '?') {
        let untracked_diff = collect_untracked_file_diff(&repo_dir, path).await;
        attach_file_diffs(
            &mut file_changes,
            "Untracked",
            &untracked_diff,
            section_output(&untracked_diff),
        );
    }

    GitDiffReport {
        repo_dir,
        file_changes,
    }
}

/// Returns the version of a text file at HEAD. New, untracked files use an
/// empty baseline, while files outside a Git worktree do not get a baseline.
pub(in crate::server) async fn file_baseline(project_dir: &Path, file: &Path) -> Option<String> {
    let repo_dir = git_worktree_root(project_dir).await.ok()?;
    let repo_dir = fs::canonicalize(repo_dir).await.ok()?;
    let canonical_file = fs::canonicalize(file).await.ok()?;
    let relative = canonical_file.strip_prefix(&repo_dir).ok()?;
    let relative = relative.to_string_lossy().replace('\\', "/");

    let tracked = run_git_command(
        &repo_dir,
        &["ls-files", "--error-unmatch", "--", relative.as_str()],
        &[0],
    )
    .await
    .is_ok();

    if tracked {
        let object = format!("HEAD:{relative}");
        return match run_git_command(&repo_dir, &["show", object.as_str()], &[0]).await {
            Ok(output) => String::from_utf8(output.stdout).ok(),
            // A file added to the index has no HEAD object yet.
            Err(_) => Some(String::new()),
        };
    }

    let untracked = run_git_command(
        &repo_dir,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--",
            relative.as_str(),
        ],
        &[0],
    )
    .await
    .ok()
    .is_some_and(|output| !output.stdout.is_empty());

    untracked.then(String::new)
}

pub(in crate::server) async fn project_is_dirty(project_dir: &Path) -> bool {
    run_git_command(
        project_dir,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
        &[0],
    )
    .await
    .is_ok_and(|output| !output.stdout.is_empty())
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

async fn collect_git_text_owned(
    project_dir: &Path,
    args: &[String],
    success_codes: &[i32],
) -> GitSection {
    let command = format!("git {}", args.join(" "));
    let output = run_git_command_owned(project_dir, args, success_codes)
        .await
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string());
    GitSection { command, output }
}

async fn collect_untracked_file_diff(project_dir: &Path, path: &str) -> GitSection {
    let args = vec![
        "diff".to_string(),
        "--no-index".to_string(),
        "--color=never".to_string(),
        "--".to_string(),
        "/dev/null".to_string(),
        path.to_string(),
    ];
    collect_git_text_owned(project_dir, &args, &[0, 1]).await
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

#[cfg(test)]
mod tests {
    use std::{fs as std_fs, process::Command, time::SystemTime};

    use super::{Path, collect_project_file_diff, file_baseline, project_is_dirty};

    fn git(directory: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(directory)
            .status()
            .expect("git should run");
        assert!(status.success(), "git {args:?} should succeed");
    }

    #[tokio::test]
    async fn reads_head_content_and_uses_empty_baseline_for_untracked_files() {
        let directory = std::env::temp_dir().join(format!(
            "latitude-file-baseline-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std_fs::create_dir_all(&directory).unwrap();
        git(&directory, &["init", "--quiet"]);
        git(&directory, &["config", "user.name", "Latitude Tests"]);
        git(
            &directory,
            &["config", "user.email", "latitude@example.invalid"],
        );
        std_fs::write(directory.join("tracked.txt"), "before\n").unwrap();
        git(&directory, &["add", "tracked.txt"]);
        git(&directory, &["commit", "--quiet", "-m", "initial"]);
        assert!(!project_is_dirty(&directory).await);

        std_fs::write(directory.join("tracked.txt"), "after\n").unwrap();
        std_fs::write(directory.join("new.txt"), "new\n").unwrap();
        assert!(project_is_dirty(&directory).await);

        let tracked = collect_project_file_diff(&directory, "tracked.txt").await;
        assert_eq!(tracked.file_changes.len(), 1);
        assert_eq!(tracked.file_changes[0].path, "tracked.txt");
        assert!(
            tracked.file_changes[0]
                .diffs
                .iter()
                .any(|diff| diff.label == "Unstaged")
        );
        assert!(
            tracked
                .file_changes
                .iter()
                .all(|change| change.path != "new.txt")
        );

        assert_eq!(
            file_baseline(&directory, &directory.join("tracked.txt")).await,
            Some("before\n".to_string())
        );
        assert_eq!(
            file_baseline(&directory, &directory.join("new.txt")).await,
            Some(String::new())
        );

        std_fs::remove_dir_all(directory).unwrap();
    }
}
