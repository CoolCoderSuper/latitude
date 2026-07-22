use maud::{Markup, html};

use crate::{
    config::{BootConfig, ProjectConfig},
    storage::WorktreeRecord,
};

use super::super::{
    assets::{HTMX_SCRIPT_SRC, PROJECT_HOME_SCRIPT_SRC, PROJECT_HOME_STYLE_HREF},
    constants::{
        DESKTOP_ROUTE_SEGMENT, DIFF_ROUTE_SEGMENT, FILES_ROUTE_SEGMENT, TERMINAL_ROUTE_SEGMENT,
    },
    git::GitStatusSummary,
    html as html_page,
    paths::display_path,
    presentation::{deployment_home_label, deployment_page_title, project_summary},
};

pub(in crate::server) fn render_project_home(
    project: &ProjectConfig,
    git_status: &GitStatusSummary,
    t3code_enabled: bool,
    device_hostname: &str,
) -> String {
    let page_title = format!("{} - Latitude Project", project.name);
    let enabled_deployments = project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .collect::<Vec<_>>();

    html_page::document(
        &page_title,
        device_hostname,
        PROJECT_HOME_STYLE_HREF,
        html! { script src=(HTMX_SCRIPT_SRC) {} },
        html! {
            main data-project-shell data-project=(&project.name) {
                header {
                    a class="back-link" href="/" { "Back to projects" }
                    h1 { (&project.name) }
                    p { "Project tools and deployments on " (device_hostname) }
                }
                ul {
                    (tool_link(&project.name, FILES_ROUTE_SEGMENT, "Files", "Browse, preview, and edit project files"))
                    (code_changes_tool_link(&project.name, git_status))
                    (tool_link(&project.name, TERMINAL_ROUTE_SEGMENT, "Terminal", "Run commands in the project directory"))
                    @if t3code_enabled {
                        li data-t3code-open { a href=(format!("/__latitude/t3code/{}", project.name)) target="_blank" rel="noopener" {
                            strong { "Open in T3 Code" }
                            span { "Start a coding agent in this repository" }
                        } }
                    }
                    @for deployment in enabled_deployments {
                        li class="deployment-item" {
                            a class="deployment-link" href=(format!("/{}/{}", project.name, deployment.name)) {
                                strong { (&deployment.name) }
                                span {
                                    (deployment_home_label(deployment))
                                    @if let Some(title) = deployment_page_title(deployment) { ": " (title) }
                                }
                            }
                            button
                                class="share-trigger"
                                type="button"
                                data-share-trigger
                                data-deployment=(&deployment.name)
                                hx-get=(format!("/__latitude/ui/shares/{}/{}", project.name, deployment.name))
                                hx-target="[data-share-dialog-shell]"
                                hx-swap="outerHTML"
                                aria-label=(format!("Manage shares for {}", deployment.name))
                                title="Manage share links" { "Share" }
                        }
                    }
                    @if project.deployments.iter().all(|deployment| !deployment.enabled) {
                        li class="empty" { "No enabled deployments yet." }
                    }
                }
                dialog class="share-dialog" data-share-dialog {
                    div class="share-dialog-shell" data-share-dialog-shell {
                        div class="share-dialog-header" {
                            h2 { "Share deployment" }
                            button class="share-close" type="button" data-share-close aria-label="Close share manager" { "×" }
                        }
                        div class="share-list" { "Loading…" }
                    }
                }
                script src=(PROJECT_HOME_SCRIPT_SRC) {}
            }
        },
    )
}

pub(in crate::server) fn render_server_home(
    config: &BootConfig,
    projects: &[ProjectConfig],
    git_statuses: &std::collections::HashMap<String, GitStatusSummary>,
    worktrees: &[WorktreeRecord],
    device_hostname: &str,
) -> String {
    let enabled_projects = projects
        .iter()
        .filter(|project| project.enabled)
        .collect::<Vec<_>>();
    let no_enabled_projects = enabled_projects.is_empty();
    let project_groups = group_projects(enabled_projects, worktrees);

    html_page::document(
        "Latitude Projects",
        device_hostname,
        PROJECT_HOME_STYLE_HREF,
        html! { script src=(HTMX_SCRIPT_SRC) {} },
        html! {
            main data-server-shell {
                header { h1 { "Latitude" } p { "Available projects on " (device_hostname) } }
                ul
                    id="project-list"
                    data-project-list
                    hx-get="/"
                    hx-trigger="every 1s, worktreeArchived from:body"
                    hx-select="[data-project-list]"
                    hx-target="#project-list"
                    hx-sync="this:drop"
                    hx-swap="outerHTML" {
                    @if config.desktop.enabled {
                        li { a href=(format!("/{DESKTOP_ROUTE_SEGMENT}")) {
                            strong { (&config.desktop.label) }
                            span { "View the desktop over VNC" }
                        } }
                    }
                    li { a href=(format!("/{TERMINAL_ROUTE_SEGMENT}")) {
                        strong { "Root Terminal" }
                        span { "Run commands in your user directory" }
                    } }
                    @if config.t3code.enabled {
                        li data-t3code-open { a href="/__latitude/t3code" target="_blank" rel="noopener" {
                            strong { "Open T3 Code" }
                            span { "Open the coding agent workspace" }
                        } }
                    }
                    @for group in project_groups {
                        @if group.projects.len() > 1 {
                            li class="worktree-group" {
                                div class="worktree-group-header" {
                                    strong { (&group.label) }
                                    span { (group.projects.len()) " worktrees" }
                                }
                                ul class="worktree-list" {
                                    @for (project, worktree) in group.projects {
                                        (server_project_item(project, git_statuses, worktree))
                                    }
                                }
                            }
                        } @else if let Some((project, worktree)) = group.projects.first() {
                            (server_project_item(project, git_statuses, *worktree))
                        }
                    }
                    @if no_enabled_projects { li class="empty" { "No enabled projects yet." } }
                }
                script src=(PROJECT_HOME_SCRIPT_SRC) {}
            }
        },
    )
}

struct ServerProjectGroup<'a> {
    label: String,
    projects: Vec<(&'a ProjectConfig, Option<&'a WorktreeRecord>)>,
}

fn group_projects<'a>(
    projects: Vec<&'a ProjectConfig>,
    worktrees: &'a [WorktreeRecord],
) -> Vec<ServerProjectGroup<'a>> {
    let worktrees_by_project = worktrees
        .iter()
        .map(|worktree| (worktree.project_name.clone(), worktree))
        .collect::<std::collections::HashMap<_, _>>();
    let repository_labels = worktrees
        .iter()
        .filter(|worktree| !worktree.discovered)
        .map(|worktree| {
            (
                worktree
                    .common_git_dir
                    .to_string_lossy()
                    .to_ascii_lowercase(),
                worktree.project_name.clone(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut groups = Vec::<ServerProjectGroup<'a>>::new();
    let mut indexes = std::collections::HashMap::<String, usize>::new();

    for project in projects {
        let worktree = worktrees_by_project.get(&project.name).copied();
        let repository = worktree.map(|worktree| {
            worktree
                .common_git_dir
                .to_string_lossy()
                .to_ascii_lowercase()
        });
        let key = repository
            .clone()
            .unwrap_or_else(|| format!("project:{}", project.name));
        if let Some(index) = indexes.get(&key).copied() {
            groups[index].projects.push((project, worktree));
            continue;
        }
        indexes.insert(key.clone(), groups.len());
        groups.push(ServerProjectGroup {
            label: repository_labels
                .get(&key)
                .cloned()
                .unwrap_or_else(|| project.name.clone()),
            projects: vec![(project, worktree)],
        });
    }
    groups
}

fn server_project_item(
    project: &ProjectConfig,
    git_statuses: &std::collections::HashMap<String, GitStatusSummary>,
    worktree: Option<&WorktreeRecord>,
) -> Markup {
    let label = worktree
        .filter(|worktree| worktree.discovered)
        .and_then(|worktree| worktree.branch.as_deref())
        .unwrap_or(&project.name);
    let description = worktree
        .filter(|worktree| worktree.discovered)
        .map(|worktree| display_path(&worktree.worktree_dir))
        .unwrap_or_else(|| project_summary(project));

    html! {
        li class="project-item" data-worktree-item=[worktree.map(|_| project.name.as_str())] {
            a href=(format!("/{}", project.name)) {
                strong {
                    span class="project-name" { (label) }
                    span
                        id=(format!("project-git-status-{}", project.name))
                        data-project-git-status=(&project.name)
                        hx-preserve {
                        @if let Some(status) = git_statuses.get(&project.name).filter(|status| status.has_status()) {
                            (git_status_badge(status))
                        }
                    }
                }
                span { (description) }
            }
            @if worktree.is_some_and(|worktree| worktree.discovered) {
                button
                    class="worktree-archive"
                    type="button"
                    hx-patch=(format!("/__latitude/ui/projects/{}/archive", project.name))
                    hx-confirm=(format!("Archive {label}? It will be hidden from the project list. Its files and Git branch will not be changed."))
                    hx-swap="none"
                    hx-disabled-elt="this"
                    aria-label=(format!("Archive {label}"))
                    title="Hide this worktree without changing its files or branch" {
                        svg
                            viewBox="0 0 24 24"
                            width="16"
                            height="16"
                            aria-hidden="true"
                            focusable="false" {
                                path d="M4 7h16v13H4zM3 3h18v4H3zm6 8h6" {}
                            }
                }
            }
        }
    }
}

fn code_changes_tool_link(project: &str, status: &GitStatusSummary) -> Markup {
    html! { li { a href=(format!("/{project}/{DIFF_ROUTE_SEGMENT}")) {
        strong {
            "Code changes"
            span data-project-git-status=(project) {
                @if status.has_status() { (git_status_badge(status)) }
            }
        }
        span { "Review changes, commits, and history" }
    } } }
}

fn git_status_badge(status: &GitStatusSummary) -> Markup {
    html! {
        span class="git-status" aria-label=(status.accessible_label()) title=(status.accessible_label()) {
            @if status.is_dirty() {
                @if status.additions > 0 {
                    span class="git-stat git-additions" { "+" (status.additions) }
                }
                @if status.deletions > 0 {
                    span class="git-stat git-deletions" { "-" (status.deletions) }
                }
                @if status.additions == 0 && status.deletions == 0 {
                    span class="git-stat" { "changed" }
                }
            }
            @if status.behind > 0 {
                span class="git-stat git-behind" title="Commits to pull" { "↓" (status.behind) }
            }
            @if status.ahead > 0 {
                span class="git-stat git-ahead" title="Commits to push" { "↑" (status.ahead) }
            }
        }
    }
}

fn tool_link(project: &str, segment: &str, label: &str, description: &str) -> Markup {
    html! { li { a href=(format!("/{project}/{segment}")) {
        strong { (label) }
        span { (description) }
    } } }
}
