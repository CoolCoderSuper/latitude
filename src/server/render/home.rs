use maud::{Markup, html};

use crate::config::{BootConfig, ProjectConfig};

use super::super::{
    assets::{HTMX_SCRIPT_SRC, PROJECT_HOME_SCRIPT_SRC, PROJECT_HOME_STYLE_HREF},
    constants::{
        DESKTOP_ROUTE_SEGMENT, DIFF_ROUTE_SEGMENT, FILES_ROUTE_SEGMENT, TERMINAL_ROUTE_SEGMENT,
    },
    html as html_page,
    presentation::{deployment_home_label, deployment_page_title, project_summary},
};

pub(in crate::server) fn render_project_home(
    project: &ProjectConfig,
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
                    (tool_link(&project.name, DIFF_ROUTE_SEGMENT, "Code changes", "Review staged and unstaged files"))
                    (tool_link(&project.name, TERMINAL_ROUTE_SEGMENT, "Terminal", "Run commands in the project directory"))
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
    dirty_projects: &[String],
    device_hostname: &str,
) -> String {
    let enabled_projects = projects
        .iter()
        .filter(|project| project.enabled)
        .collect::<Vec<_>>();
    let no_enabled_projects = enabled_projects.is_empty();

    html_page::document(
        "Latitude Projects",
        device_hostname,
        PROJECT_HOME_STYLE_HREF,
        html! {},
        html! {
            main {
                header { h1 { "Latitude" } p { "Available projects on " (device_hostname) } }
                ul {
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
                    @for project in enabled_projects {
                        li { a href=(format!("/{}", project.name)) {
                            strong {
                                span class="project-name" { (&project.name) }
                                @if dirty_projects.contains(&project.name) {
                                    span class="git-dirty" title="Uncommitted Git changes" { "Dirty" }
                                }
                            }
                            span { (project_summary(project)) }
                        } }
                    }
                    @if no_enabled_projects { li class="empty" { "No enabled projects yet." } }
                }
            }
        },
    )
}

fn tool_link(project: &str, segment: &str, label: &str, description: &str) -> Markup {
    html! { li { a href=(format!("/{project}/{segment}")) {
        strong { (label) }
        span { (description) }
    } } }
}
