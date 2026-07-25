use std::path::Path;

use tokio::{
    fs,
    io::{AsyncReadExt, BufReader},
};

use super::{
    command::{
        GitCommandExecution, git_command_label, git_worktree_root, parse_nul_separated_paths,
        run_git_command, run_git_command_owned, run_git_command_with_execution,
    },
    types::{
        GitCommit, GitCommitReport, GitDiffReport, GitFileChange, GitFileDiff, GitHistoryReport,
        GitSection, GitStatusSummary,
    },
};

pub(in crate::server) async fn collect_project_diff(project_dir: &Path) -> GitDiffReport {
    let status_summary = collect_project_git_status(project_dir).await;
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
            status: status_summary,
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
        status: status_summary,
        file_changes,
    }
}

pub(in crate::server) async fn collect_project_file_diff(
    project_dir: &Path,
    path: &str,
) -> GitDiffReport {
    let status_summary = collect_project_git_status(project_dir).await;
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
            status: status_summary,
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
        status: status_summary,
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

pub(in crate::server) async fn collect_project_git_status(project_dir: &Path) -> GitStatusSummary {
    collect_project_git_status_with_execution(project_dir, GitCommandExecution::Interactive).await
}

pub(in crate::server) async fn collect_project_git_status_with_execution(
    project_dir: &Path,
    execution: GitCommandExecution,
) -> GitStatusSummary {
    // Catalog project directories are normalized to worktree roots during discovery, so status
    // refreshes can run there directly without a separate `git rev-parse` process per project.
    let repo_dir = project_dir;
    let mut summary = GitStatusSummary::default();

    let status_future = run_git_command_with_execution(
        repo_dir,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=normal",
        ],
        &[0],
        execution,
    );
    let diff_future = run_git_command_with_execution(
        repo_dir,
        &["diff", "HEAD", "--numstat", "--no-renames"],
        &[0],
        execution,
    );
    let (status, diff) = tokio::join!(status_future, diff_future);

    match diff {
        Ok(output) => add_numstat(&mut summary, &String::from_utf8_lossy(&output.stdout)),
        Err(_) => {
            let (cached, unstaged) = tokio::join!(
                run_git_command_with_execution(
                    repo_dir,
                    &["diff", "--cached", "--numstat", "--no-renames"],
                    &[0],
                    execution,
                ),
                run_git_command_with_execution(
                    repo_dir,
                    &["diff", "--numstat", "--no-renames", "--"],
                    &[0],
                    execution,
                )
            );
            for output in [cached, unstaged].into_iter().flatten() {
                add_numstat(&mut summary, &String::from_utf8_lossy(&output.stdout));
            }
        }
    }

    if let Ok(output) = status {
        for path in apply_porcelain_v2_status(&mut summary, &output.stdout) {
            summary.additions += untracked_text_line_count(&repo_dir.join(path)).await;
        }
    }
    summary
}

fn apply_porcelain_v2_status(summary: &mut GitStatusSummary, output: &[u8]) -> Vec<String> {
    let mut untracked = Vec::new();
    for entry in output.split(|byte| *byte == 0) {
        if let Some(counts) = entry.strip_prefix(b"# branch.ab ") {
            let counts = String::from_utf8_lossy(counts);
            for value in counts.split_whitespace() {
                if let Some(ahead) = value.strip_prefix('+') {
                    summary.ahead = ahead.parse().unwrap_or(0);
                } else if let Some(behind) = value.strip_prefix('-') {
                    summary.behind = behind.parse().unwrap_or(0);
                }
            }
        } else if let Some(path) = entry.strip_prefix(b"? ") {
            summary.dirty = true;
            untracked.push(String::from_utf8_lossy(path).into_owned());
        } else if matches!(entry.first(), Some(b'1' | b'2' | b'u')) {
            summary.dirty = true;
        }
    }
    untracked
}

async fn untracked_text_line_count(path: &Path) -> usize {
    let Ok(file) = fs::File::open(path).await else {
        return 0;
    };
    let mut reader = BufReader::new(file);
    let mut buffer = [0_u8; 16 * 1024];
    let mut lines = 0;
    let mut saw_bytes = false;
    let mut ended_with_newline = false;

    loop {
        let Ok(read) = reader.read(&mut buffer).await else {
            return 0;
        };
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        if chunk.contains(&0) {
            return 0;
        }
        saw_bytes = true;
        lines += chunk.iter().filter(|byte| **byte == b'\n').count();
        ended_with_newline = chunk.last() == Some(&b'\n');
    }

    lines + usize::from(saw_bytes && !ended_with_newline)
}

fn add_numstat(summary: &mut GitStatusSummary, output: &str) {
    for line in output.lines() {
        let mut fields = line.split('\t');
        summary.additions += fields
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        summary.deletions += fields
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
    }
}

pub(in crate::server) async fn collect_project_git_history(project_dir: &Path) -> GitHistoryReport {
    let fallback_dir = fs::canonicalize(project_dir)
        .await
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let repo_dir = git_worktree_root(project_dir)
        .await
        .unwrap_or_else(|_| fallback_dir.clone());
    let output = run_git_command(
        &repo_dir,
        &[
            "log",
            "-n",
            "30",
            "--date=iso-strict",
            "--format=%x1e%H%x1f%h%x1f%an%x1f%ad%x1f%s",
        ],
        &[0],
    )
    .await;
    let commits = output
        .ok()
        .map(|output| parse_git_history(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default();
    GitHistoryReport { repo_dir, commits }
}

pub(in crate::server) async fn collect_project_git_commit(
    project_dir: &Path,
    hash: &str,
) -> Option<GitCommitReport> {
    if hash.len() != 40 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let repo_dir = git_worktree_root(project_dir).await.ok()?;
    let args = vec![
        "show".to_string(),
        "--date=iso-strict".to_string(),
        "--format=%x1e%H%x1f%h%x1f%an%x1f%ad%x1f%s".to_string(),
        "--patch".to_string(),
        "--no-ext-diff".to_string(),
        "--color=never".to_string(),
        hash.to_string(),
    ];
    let output = run_git_command_owned(&repo_dir, &args, &[0]).await.ok()?;
    let commit = parse_git_history(&String::from_utf8_lossy(&output.stdout))
        .into_iter()
        .next()?;
    (commit.hash.eq_ignore_ascii_case(hash)).then_some(GitCommitReport { repo_dir, commit })
}

fn parse_git_history(output: &str) -> Vec<GitCommit> {
    output
        .split('\u{1e}')
        .filter_map(|record| {
            let record = record.trim_start_matches(['\r', '\n']);
            let (metadata, diff) = record.split_once('\n').unwrap_or((record, ""));
            let mut fields = metadata.trim_end().split('\u{1f}');
            let diff = diff.trim().to_string();
            let files = parse_diff_file_sections("Commit", "git show", &diff);
            Some(GitCommit {
                hash: fields.next()?.to_string(),
                short_hash: fields.next()?.to_string(),
                author: fields.next()?.to_string(),
                authored_at: fields.next()?.to_string(),
                subject: fields.next()?.to_string(),
                diff,
                files,
            })
        })
        .collect()
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

    use super::{
        GitStatusSummary, Path, apply_porcelain_v2_status, collect_project_file_diff,
        collect_project_git_commit, collect_project_git_history, collect_project_git_status,
        file_baseline,
    };

    fn git(directory: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(directory)
            .status()
            .expect("git should run");
        assert!(status.success(), "git {args:?} should succeed");
    }

    #[test]
    fn parses_porcelain_v2_status_and_branch_counts() {
        let mut summary = GitStatusSummary::default();
        let untracked = apply_porcelain_v2_status(
            &mut summary,
            b"# branch.oid abc\0# branch.ab +3 -2\01 .M N... tracked.txt\0? new.txt\0",
        );

        assert!(summary.dirty);
        assert_eq!(summary.ahead, 3);
        assert_eq!(summary.behind, 2);
        assert_eq!(untracked, ["new.txt"]);
    }

    #[test]
    fn porcelain_v2_branch_headers_alone_are_clean() {
        let mut summary = GitStatusSummary::default();
        let untracked = apply_porcelain_v2_status(
            &mut summary,
            b"# branch.oid abc\0# branch.head master\0# branch.ab +0 -0\0",
        );

        assert!(!summary.dirty);
        assert!(untracked.is_empty());
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
        assert!(!collect_project_git_status(&directory).await.is_dirty());
        let history = collect_project_git_history(&directory).await;
        assert_eq!(history.commits.len(), 1);
        assert_eq!(history.commits[0].subject, "initial");
        assert!(history.commits[0].diff.is_empty());
        let commit = collect_project_git_commit(&directory, &history.commits[0].hash)
            .await
            .expect("history commit should be readable");
        assert!(commit.commit.diff.contains("tracked.txt"));

        std_fs::write(directory.join("tracked.txt"), "after\n").unwrap();
        std_fs::write(directory.join("new.txt"), "new\n").unwrap();
        let status = collect_project_git_status(&directory).await;
        assert!(status.is_dirty());
        assert_eq!(status.additions, 2);
        assert_eq!(status.deletions, 1);

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
