use maud::{Markup, PreEscaped, html};

use crate::config::ProjectConfig;

use super::{
    super::{
        assets::{DIFF_VIEWER_SCRIPT_SRC, DIFF_VIEWER_STYLE_HREF, HTMX_SCRIPT_SRC},
        constants::DIFF_ROUTE_SEGMENT,
        git::{
            FileSectionKind, GitCommitReport, GitDiffReport, GitFileChange, GitFileDiff,
            GitHistoryReport,
        },
        html as html_page,
        paths::display_path,
    },
    syntax::render_diff_code_output,
};

pub(in crate::server) fn render_diff_workspace_fragment(
    report: &GitDiffReport,
    action_url: &str,
) -> Markup {
    diff_workspace_inner(report, action_url)
}

pub(in crate::server) fn render_project_git_history(
    project: &ProjectConfig,
    report: &GitHistoryReport,
    device_hostname: &str,
) -> String {
    let page_title = format!("{} Git history - Latitude", project.name);
    html_page::document(
        &page_title,
        device_hostname,
        DIFF_VIEWER_STYLE_HREF,
        html! {},
        html! {
            main {
                header {
                    a href=(format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT)) { "Back to code changes" }
                    h1 { "Git history" }
                    p { (&project.name) " on " (device_hostname) }
                    p class="project-path" { (display_path(&report.repo_dir)) }
                }
                section class="history-panel" {
                    @if report.commits.is_empty() {
                        div class="empty" { "No commits found." }
                    } @else {
                        @for commit in &report.commits {
                            a class="history-commit" href=(format!("/{}/{}/history/{}", project.name, DIFF_ROUTE_SEGMENT, commit.hash)) {
                                div class="history-summary" {
                                    code { (&commit.short_hash) }
                                    strong { (&commit.subject) }
                                    span { (&commit.author) " · " (&commit.authored_at) }
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

pub(in crate::server) fn render_project_git_commit(
    project: &ProjectConfig,
    report: &GitCommitReport,
    device_hostname: &str,
) -> String {
    let commit = &report.commit;
    let (additions, deletions) = commit.files.iter().fold((0, 0), |totals, file| {
        let counts = diff_line_counts(&file.content);
        (totals.0 + counts.0, totals.1 + counts.1)
    });
    let page_title = format!("{} {} - Latitude", project.name, commit.short_hash);
    html_page::document(
        &page_title,
        device_hostname,
        DIFF_VIEWER_STYLE_HREF,
        html! {},
        html! {
            main {
                header {
                    a href=(format!("/{}/{}/history", project.name, DIFF_ROUTE_SEGMENT)) { "Back to Git history" }
                    h1 { (&commit.subject) }
                    p { (&project.name) " on " (device_hostname) }
                    p class="project-path" { (display_path(&report.repo_dir)) }
                }
                section class="commit-header" {
                    div class="commit-meta" {
                        code title=(&commit.hash) { (&commit.short_hash) }
                        " · " (&commit.author)
                        " · " (&commit.authored_at)
                    }
                    div class="commit-summary" aria-label=(format!("{} files changed, {additions} additions, {deletions} deletions", commit.files.len())) {
                        span { (file_count_label(commit.files.len())) " changed" }
                        span class="commit-additions" { "+" (additions) }
                        span class="commit-deletions" { "-" (deletions) }
                    }
                }
                section class="commit-file-list" {
                    @if commit.files.is_empty() {
                        div class="empty" { "This commit has no textual file diff." }
                    } @else {
                        @for file in &commit.files {
                            (commit_file_card(file))
                        }
                    }
                }
            }
        },
    )
}

fn commit_file_card(file: &GitFileDiff) -> Markup {
    let (additions, deletions) = diff_line_counts(&file.content);
    html! {
        details class="file-card commit-file-card" {
            summary class="file-summary commit-file-summary" {
                div class="file-path" { (&file.path) }
                div class="commit-file-stats" aria-label=(format!("{additions} additions, {deletions} deletions")) {
                    span class="commit-additions" { "+" (additions) }
                    span class="commit-deletions" { "-" (deletions) }
                }
            }
            div class="file-content" { (git_file_diff(file)) }
        }
    }
}

fn diff_line_counts(content: &str) -> (usize, usize) {
    let additions = content
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .count();
    let deletions = content
        .lines()
        .filter(|line| line.starts_with('-') && !line.starts_with("---"))
        .count();
    (additions, deletions)
}

pub(in crate::server) fn render_diff_file_update(
    report: &GitDiffReport,
    path: &str,
    action_url: &str,
) -> Markup {
    let change = report
        .file_changes
        .iter()
        .find(|change| change.path == path || change.original_path.as_deref() == Some(path));
    html! {
        div data-diff-file-update data-path=(path) {
            @for kind in [FileSectionKind::Unstaged, FileSectionKind::Staged] {
                template data-file-section-update=(kind.data_key()) {
                    @if let Some(change) = change.filter(|change| kind.includes(change)) {
                        (git_file_card(change, kind, action_url))
                    }
                }
            }
        }
    }
}

pub(in crate::server) fn render_project_diff(
    project: &ProjectConfig,
    report: &GitDiffReport,
    device_hostname: &str,
) -> String {
    let page_title = format!("{} code changes - Latitude", project.name);
    let action_url = format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT);

    html_page::document(
        &page_title,
        device_hostname,
        DIFF_VIEWER_STYLE_HREF,
        html! { script src=(HTMX_SCRIPT_SRC) {} },
        html! {
            main {
                header {
                    a href=(format!("/{}", project.name)) { "Back to project" }
                    h1 { "Code changes" }
                    p { (&project.name) " on " (device_hostname) }
                    p class="project-path" { (display_path(&report.repo_dir)) }
                }
                div class="diff-workspace" data-diff-workspace data-action-url=(&action_url) {
                    (render_diff_workspace_fragment(report, &action_url))
                }
                script src=(DIFF_VIEWER_SCRIPT_SRC) {}
            }
        },
    )
}

fn diff_workspace_inner(report: &GitDiffReport, action_url: &str) -> Markup {
    html! {
        section class="git-overview" aria-label="Git change and sync totals" {
            strong { "Changes" }
            @if report.status.additions > 0 {
                span class="overview-additions" { "+" (report.status.additions) }
            }
            @if report.status.deletions > 0 {
                span class="overview-deletions" { "-" (report.status.deletions) }
            }
            span class="overview-spacer" {}
            @if report.status.behind > 0 {
                span class="overview-behind" { "↓" (report.status.behind) " pull" }
            }
            @if report.status.ahead > 0 {
                span class="overview-ahead" { "↑" (report.status.ahead) " push" }
            }
        }
        div class="action-status" data-action-status hidden {}
        (git_action_panel(action_url))
        (git_file_panel(&report.file_changes, action_url))
    }
}

fn git_action_panel(action_url: &str) -> Markup {
    html! {
        section class="action-panel" {
            div class="action-group" {
                (git_stage_action_button(action_url))
                (git_unstage_action_button(action_url))
                (git_destructive_action_button(
                    action_url,
                    "discard_all",
                    "Discard all",
                    "Discard all unstaged changes and untracked files? This cannot be undone.",
                ))
            }
            form class="commit-form" hx-patch=(action_url) hx-swap="none" {
                input data-commit-message name="message" type="text" required placeholder="Commit message";
                button type="submit" name="action" value="commit" data-git-action="commit" { "Commit staged" }
            }
            div class="action-group action-group-push" {
                a class="editor-link" href=(format!("{action_url}/history")) { "History" }
                (git_action_button(action_url, "pull", "Pull"))
                (git_action_button(action_url, "push", "Push"))
            }
        }
    }
}

fn git_action_button(action_url: &str, action: &str, label: &str) -> Markup {
    html! {
        form class="git-action-form" hx-patch=(action_url) hx-swap="none" {
            button type="submit" name="action" value=(action) data-git-action=(action) { (label) }
        }
    }
}

fn git_stage_action_button(action_url: &str) -> Markup {
    html! {
        form id="stage-action-form" class="git-action-form" hx-patch=(action_url) hx-swap="none" {
            button type="submit" name="action" value="stage_all" data-git-action="stage_all" data-stage-action { "Stage all" }
        }
    }
}

fn git_unstage_action_button(action_url: &str) -> Markup {
    html! {
        form id="unstage-action-form" class="git-action-form" hx-patch=(action_url) hx-swap="none" {
            button type="submit" name="action" value="unstage_all" data-git-action="unstage_all" data-unstage-action { "Unstage all" }
        }
    }
}

fn git_file_panel(changes: &[GitFileChange], action_url: &str) -> Markup {
    html! {
        (git_file_section(
            "Unstaged files",
            "No unstaged files.",
            changes,
            FileSectionKind::Unstaged,
            action_url,
        ))
        (git_file_section(
            "Staged files",
            "No staged files.",
            changes,
            FileSectionKind::Staged,
            action_url,
        ))
    }
}

fn git_file_section(
    title: &str,
    empty_message: &str,
    changes: &[GitFileChange],
    kind: FileSectionKind,
    action_url: &str,
) -> Markup {
    let section_changes = changes
        .iter()
        .filter(|change| kind.includes(change))
        .collect::<Vec<_>>();
    let count_label = file_count_label(section_changes.len());

    html! {
        section class="file-panel" data-file-panel=(kind.data_key()) data-empty-message=(empty_message) {
            div class="section-heading" {
                h2 { (title) }
                code { (count_label) }
            }
            @if section_changes.is_empty() {
                div class="empty" { (empty_message) }
            } @else {
                div class="file-list" {
                    @for change in &section_changes {
                        (git_file_card(change, kind, action_url))
                    }
                }
            }
        }
    }
}

fn git_file_card(change: &GitFileChange, kind: FileSectionKind, action_url: &str) -> Markup {
    let visible_diffs = change
        .diffs
        .iter()
        .filter(|diff| kind.includes_diff(diff))
        .collect::<Vec<_>>();
    let (selection_form, selection_action) = match kind {
        FileSectionKind::Unstaged => ("stage-action-form", "staging"),
        FileSectionKind::Staged => ("unstage-action-form", "unstaging"),
    };

    html! {
        details class="file-card" data-file-section=(kind.data_key()) data-file-path=(&change.path) {
            summary class="file-summary" {
                input class="file-select" type="checkbox" value=(&change.path) form=(selection_form) data-file-select data-selection-kind=(kind.data_key()) aria-label=(format!("Select {} for {selection_action}", change.path));
                div class="status-code" { (change.status_label()) }
                div class="file-path" {
                    (&change.path)
                    @if let Some(original_path) = &change.original_path {
                        span { " from " (original_path) }
                    }
                }
                div class="file-summary-action" {
                    @if change.can_open_in_editor() {
                        a class="editor-link" data-open-editor href=(editor_href(&change.path)) target="_blank" rel="noopener" { "Open in editor" }
                    }
                    @match kind {
                        FileSectionKind::Unstaged => {
                            (git_file_action_button(action_url, "stage_file", "Stage", &change.path))
                            (git_file_destructive_action_button(
                                action_url,
                                "discard_file",
                                "Discard",
                                &change.path,
                                "Discard unstaged changes for this file? This cannot be undone.",
                            ))
                        }
                        FileSectionKind::Staged => {
                            (git_file_action_button(action_url, "unstage_file", "Unstage", &change.path))
                        }
                    }
                }
            }
            div class="file-content" {
                @if visible_diffs.is_empty() {
                    div class="empty" { "No inline diff for this file." }
                } @else {
                    @for diff in visible_diffs {
                        (git_file_diff(diff))
                    }
                }
            }
        }
    }
}

fn editor_href(path: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("path", path)
        .finish();
    format!("./_files?{query}")
}

fn git_destructive_action_button(
    action_url: &str,
    action: &str,
    label: &str,
    confirm: &str,
) -> Markup {
    html! {
        form class="git-action-form" hx-patch=(action_url) hx-swap="none" hx-confirm=(confirm) {
            button class="danger-button" type="submit" name="action" value=(action) data-git-action=(action) { (label) }
        }
    }
}

fn git_file_action_button(action_url: &str, action: &str, label: &str, path: &str) -> Markup {
    html! {
        form class="git-action-form" hx-patch=(action_url) hx-swap="none" {
            input type="hidden" name="path" value=(path);
            button type="submit" name="action" value=(action) data-git-action=(action) data-path=(path) { (label) }
        }
    }
}

fn git_file_destructive_action_button(
    action_url: &str,
    action: &str,
    label: &str,
    path: &str,
    confirm: &str,
) -> Markup {
    html! {
        form class="git-action-form" hx-patch=(action_url) hx-swap="none" hx-confirm=(confirm) {
            input type="hidden" name="path" value=(path);
            button class="danger-button" type="submit" name="action" value=(action) data-git-action=(action) data-path=(path) { (label) }
        }
    }
}

fn git_file_diff(diff: &GitFileDiff) -> Markup {
    html! {
        div class="file-diff" {
            (PreEscaped(diff_code_html(&diff.content, &diff.path)))
        }
    }
}

fn diff_code_html(content: &str, path: &str) -> String {
    let mut output = String::new();
    render_diff_code_output(&mut output, content, path);
    output
}

fn file_count_label(count: usize) -> String {
    match count {
        1 => "1 file".to_string(),
        count => format!("{count} files"),
    }
}
