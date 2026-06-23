use maud::{Markup, PreEscaped, html};

use super::{
    super::git::{FileSectionKind, GitDiffReport, GitFileChange, GitFileDiff},
    syntax::render_diff_code_output,
};

pub(in crate::server) fn render_diff_workspace_fragment(report: &GitDiffReport) -> String {
    diff_workspace_inner(report).into_string()
}

fn diff_workspace_inner(report: &GitDiffReport) -> Markup {
    html! {
        div class="action-status" data-action-status hidden {}
        (git_action_panel())
        (git_file_panel(&report.file_changes))
    }
}

fn git_action_panel() -> Markup {
    html! {
        section class="action-panel" {
            div class="action-group" {
                (git_action_button("stage_all", "Stage all"))
                (git_action_button("unstage_all", "Unstage all"))
                (git_destructive_action_button(
                    "discard_all",
                    "Discard all",
                    "Discard all unstaged changes and untracked files? This cannot be undone.",
                ))
            }
            div class="commit-form" {
                input data-commit-message type="text" required placeholder="Commit message";
                button type="button" data-git-action="commit" { "Commit staged" }
            }
            div class="action-group action-group-push" {
                (git_action_button("push", "Push"))
            }
        }
    }
}

fn git_action_button(action: &str, label: &str) -> Markup {
    html! {
        button type="button" data-git-action=(action) { (label) }
    }
}

fn git_file_panel(changes: &[GitFileChange]) -> Markup {
    html! {
        (git_file_section(
            "Unstaged files",
            "No unstaged files.",
            changes,
            FileSectionKind::Unstaged,
        ))
        (git_file_section(
            "Staged files",
            "No staged files.",
            changes,
            FileSectionKind::Staged,
        ))
    }
}

fn git_file_section(
    title: &str,
    empty_message: &str,
    changes: &[GitFileChange],
    kind: FileSectionKind,
) -> Markup {
    let section_changes = changes
        .iter()
        .filter(|change| kind.includes(change))
        .collect::<Vec<_>>();
    let count_label = file_count_label(section_changes.len());

    html! {
        section class="file-panel" {
            div class="section-heading" {
                h2 { (title) }
                code { (count_label) }
            }
            @if section_changes.is_empty() {
                div class="empty" { (empty_message) }
            } @else {
                div class="file-list" {
                    @for change in &section_changes {
                        (git_file_card(change, kind))
                    }
                }
            }
        }
    }
}

fn git_file_card(change: &GitFileChange, kind: FileSectionKind) -> Markup {
    let visible_diffs = change
        .diffs
        .iter()
        .filter(|diff| kind.includes_diff(diff))
        .collect::<Vec<_>>();

    html! {
        details class="file-card" data-file-path=(&change.path) {
            summary class="file-summary" {
                div class="status-code" { (change.status_label()) }
                div class="file-path" {
                    (&change.path)
                    @if let Some(original_path) = &change.original_path {
                        span { " from " (original_path) }
                    }
                }
                div class="file-summary-action" {
                    @match kind {
                        FileSectionKind::Unstaged => {
                            (git_file_action_button("stage_file", "Stage", &change.path))
                            (git_file_destructive_action_button(
                                "discard_file",
                                "Discard",
                                &change.path,
                                "Discard unstaged changes for this file? This cannot be undone.",
                            ))
                        }
                        FileSectionKind::Staged => {
                            (git_file_action_button("unstage_file", "Unstage", &change.path))
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

fn git_destructive_action_button(action: &str, label: &str, confirm: &str) -> Markup {
    html! {
        button class="danger-button" type="button" data-git-action=(action) data-confirm=(confirm) { (label) }
    }
}

fn git_file_action_button(action: &str, label: &str, path: &str) -> Markup {
    html! {
        button type="button" data-git-action=(action) data-path=(path) { (label) }
    }
}

fn git_file_destructive_action_button(
    action: &str,
    label: &str,
    path: &str,
    confirm: &str,
) -> Markup {
    html! {
        button class="danger-button" type="button" data-git-action=(action) data-path=(path) data-confirm=(confirm) { (label) }
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
