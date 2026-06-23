mod diff;
mod syntax;
mod terminal;

pub(super) use diff::render_diff_workspace_fragment;

#[cfg(test)]
pub(super) use syntax::{
    SyntaxLanguage, diff_line_class, render_diff_code_output, syntax_language_for_path,
};

use maud::{PreEscaped, html};

use crate::config::{ApplicationConfig, ApplicationTarget, LatitudeConfig, ProjectConfig};

use super::{
    assets::{
        AUTH_PAGE_STYLE, DIFF_VIEWER_SCRIPT, DIFF_VIEWER_STYLE, PROJECT_HOME_STYLE,
        TERMINAL_VIEWER_SCRIPT, TERMINAL_VIEWER_STYLE,
    },
    constants::{DIFF_ROUTE_SEGMENT, LOGIN_PATH, TERMINAL_ROUTE_SEGMENT, TERMINAL_WS_SUFFIX},
    git::GitDiffReport,
    html as html_page,
    paths::display_path,
    terminal_api::PublicTerminalInfoResponse,
};

pub(super) fn render_project_home(project: &ProjectConfig) -> String {
    let page_title = format!("{} - Latitude Project", project.name);
    let enabled_deployments = project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .collect::<Vec<_>>();

    html_page::document(
        &page_title,
        PROJECT_HOME_STYLE,
        html! {},
        html! {
            main {
                h1 { (&project.name) }
                p { "Project tools and deployments" }
                ul {
                    li {
                        a href=(format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT)) {
                            strong { "Code changes" }
                            span { "Review staged and unstaged files" }
                        }
                    }
                    li {
                        a href=(format!("/{}/{}", project.name, TERMINAL_ROUTE_SEGMENT)) {
                            strong { "Terminal" }
                            span { "Run commands in the project directory" }
                        }
                    }
                    @for deployment in enabled_deployments {
                        li {
                            a href=(format!("/{}/{}", project.name, deployment.name)) {
                                strong { (&deployment.name) }
                                span {
                                    (deployment_home_label(deployment))
                                    @if let Some(title) = deployment_page_title(deployment) {
                                        ": " (title)
                                    }
                                }
                            }
                        }
                    }
                    @if project.deployments.iter().all(|deployment| !deployment.enabled) {
                        li class="empty" { "No enabled deployments yet." }
                    }
                }
            }
        },
    )
}

pub(super) fn render_project_diff(project: &ProjectConfig, report: &GitDiffReport) -> String {
    let page_title = format!("{} code changes - Latitude", project.name);
    let workspace_html = diff::render_diff_workspace_fragment(report);

    html_page::document(
        &page_title,
        DIFF_VIEWER_STYLE,
        html! {},
        html! {
            main {
                header {
                    a href=(format!("/{}", project.name)) { "Back to project" }
                    h1 { "Code changes" }
                    p { (&project.name) }
                    p class="project-path" { (display_path(&report.repo_dir)) }
                }
                div class="diff-workspace" data-diff-workspace data-action-url=(format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT)) {
                    (PreEscaped(workspace_html))
                }
                script { (PreEscaped(DIFF_VIEWER_SCRIPT)) }
            }
        },
    )
}

pub(super) fn render_project_terminal(
    project: &ProjectConfig,
    info: &PublicTerminalInfoResponse,
    websocket_token: Option<&str>,
) -> String {
    let page_title = format!("{} terminal - Latitude", project.name);
    let websocket_path = format!(
        "/{}/{}/{}",
        project.name, TERMINAL_ROUTE_SEGMENT, TERMINAL_WS_SUFFIX
    );

    html_page::document(
        &page_title,
        TERMINAL_VIEWER_STYLE,
        html! {
            link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.css";
        },
        html! {
            main {
                header {
                    a href=(format!("/{}", project.name)) { "Back to project" }
                    h1 { "Terminal" }
                    p { (&project.name) }
                    p class="project-path" { (&info.cwd) }
                }
                (terminal::terminal_workspace(info, &websocket_path, websocket_token))
                script src="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.min.js" {}
                script { (PreEscaped(TERMINAL_VIEWER_SCRIPT)) }
            }
        },
    )
}

pub(super) fn render_public_login(next: &str, login_failed: bool) -> String {
    html_page::document(
        "Sign in - Latitude",
        AUTH_PAGE_STYLE,
        html! {},
        html! {
            main {
                h1 { "Latitude" }
                p { "Sign in to continue" }
                @if login_failed {
                    div class="error" { "Incorrect password." }
                }
                form method="post" action=(LOGIN_PATH) {
                    input type="hidden" name="next" value=(next);
                    label {
                        "Password"
                        input name="password" type="password" required autofocus autocomplete="current-password";
                    }
                    button type="submit" { "Sign in" }
                }
            }
        },
    )
}

pub(super) fn render_server_home(config: &LatitudeConfig) -> String {
    let enabled_projects = config
        .projects
        .iter()
        .filter(|project| project.enabled)
        .collect::<Vec<_>>();

    html_page::document(
        "Latitude Projects",
        PROJECT_HOME_STYLE,
        html! {},
        html! {
            main {
                h1 { "Latitude" }
                p { "Available projects" }
                @if enabled_projects.is_empty() {
                    div class="empty" { "No enabled projects yet." }
                } @else {
                    ul {
                        @for project in enabled_projects {
                            li {
                                a href=(format!("/{}", project.name)) {
                                    strong { (&project.name) }
                                    span { (project_summary(project)) }
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

pub(super) fn project_summary(project: &ProjectConfig) -> String {
    let enabled_deployment_count = enabled_deployment_count(project);

    match enabled_deployment_count {
        1 => "1 deployment".to_string(),
        count => format!("{count} deployments"),
    }
}

pub(super) fn enabled_deployment_count(project: &ProjectConfig) -> usize {
    project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .count()
}

pub(super) fn deployment_kind(deployment: &ApplicationConfig) -> &'static str {
    match &deployment.target {
        ApplicationTarget::ReverseProxy { .. } => "reverse_proxy",
        ApplicationTarget::Static { .. } => "static",
        ApplicationTarget::Page { .. } => "page",
    }
}

pub(super) fn deployment_home_label(deployment: &ApplicationConfig) -> &'static str {
    match &deployment.target {
        ApplicationTarget::ReverseProxy { .. } => "Website",
        ApplicationTarget::Static { .. } => "Static website",
        ApplicationTarget::Page { .. } => "Page",
    }
}

pub(super) fn deployment_page_title(deployment: &ApplicationConfig) -> Option<&str> {
    match &deployment.target {
        ApplicationTarget::Page { title, .. } => title.as_deref(),
        _ => None,
    }
}
