use serde::Serialize;

use crate::config::{ApplicationConfig, ProjectConfig};
use crate::desktop::{DesktopScreenResponse, detect_desktop_screens};

use super::super::{
    constants::{
        DESKTOP_ROUTE_SEGMENT, DIFF_ROUTE_SEGMENT, PUBLIC_API_PROJECTS_PATH,
        PUBLIC_API_ROOT_DESKTOP_PATH, PUBLIC_API_ROOT_TERMINAL_PATH, TERMINAL_ROUTE_SEGMENT,
    },
    render::{
        deployment_home_label, deployment_kind, deployment_media_type, deployment_page_title,
        enabled_deployment_count, project_summary,
    },
};

#[derive(Debug, serde::Deserialize)]
pub(in crate::server) struct PublicLoginPayload {
    pub(in crate::server) password: String,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicSessionResponse {
    pub(in crate::server) authenticated: bool,
    pub(in crate::server) projects_href: Option<String>,
    pub(in crate::server) root_terminal: Option<PublicRootTerminalLink>,
    pub(in crate::server) root_desktop: Option<PublicRootDesktopLink>,
    pub(in crate::server) device_hostname: String,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicLoginResponse {
    pub(in crate::server) token: String,
    pub(in crate::server) max_age_seconds: u64,
    pub(in crate::server) projects_href: String,
    pub(in crate::server) root_terminal: PublicRootTerminalLink,
    pub(in crate::server) root_desktop: Option<PublicRootDesktopLink>,
    pub(in crate::server) device_hostname: String,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicProjectListResponse {
    pub(in crate::server) device_hostname: String,
    pub(in crate::server) root_terminal: PublicRootTerminalLink,
    pub(in crate::server) root_desktop: Option<PublicRootDesktopLink>,
    pub(in crate::server) projects: Vec<PublicProjectSummary>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicProjectSummary {
    pub(in crate::server) name: String,
    pub(in crate::server) href: String,
    pub(in crate::server) api_href: String,
    pub(in crate::server) summary: String,
    pub(in crate::server) deployment_count: usize,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicProjectDetail {
    pub(in crate::server) name: String,
    pub(in crate::server) device_hostname: String,
    pub(in crate::server) href: String,
    pub(in crate::server) api_href: String,
    pub(in crate::server) summary: String,
    pub(in crate::server) deployment_count: usize,
    pub(in crate::server) diff: PublicProjectDiffLink,
    pub(in crate::server) terminal: PublicProjectTerminalLink,
    pub(in crate::server) deployments: Vec<PublicDeploymentSummary>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicProjectDiffLink {
    pub(in crate::server) href: String,
    pub(in crate::server) api_href: String,
    pub(in crate::server) label: &'static str,
    pub(in crate::server) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicProjectTerminalLink {
    pub(in crate::server) href: String,
    pub(in crate::server) api_href: String,
    pub(in crate::server) label: &'static str,
    pub(in crate::server) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicRootTerminalLink {
    pub(in crate::server) href: String,
    pub(in crate::server) api_href: String,
    pub(in crate::server) label: &'static str,
    pub(in crate::server) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicRootDesktopLink {
    pub(in crate::server) href: String,
    pub(in crate::server) api_href: String,
    pub(in crate::server) label: String,
    pub(in crate::server) description: &'static str,
    pub(in crate::server) view_only: bool,
    pub(in crate::server) screens: Vec<DesktopScreenResponse>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct PublicDeploymentSummary {
    pub(in crate::server) name: String,
    pub(in crate::server) href: String,
    pub(in crate::server) kind: &'static str,
    pub(in crate::server) label: &'static str,
    pub(in crate::server) media_type: Option<String>,
    pub(in crate::server) title: Option<String>,
}

pub(in crate::server::public) fn public_project_summary(
    project: &ProjectConfig,
) -> PublicProjectSummary {
    let deployment_count = enabled_deployment_count(project);
    PublicProjectSummary {
        name: project.name.clone(),
        href: format!("/{}", project.name),
        api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}", project.name),
        summary: project_summary(project),
        deployment_count,
    }
}

pub(in crate::server) fn public_root_terminal_link() -> PublicRootTerminalLink {
    PublicRootTerminalLink {
        href: format!("/{TERMINAL_ROUTE_SEGMENT}"),
        api_href: PUBLIC_API_ROOT_TERMINAL_PATH.to_string(),
        label: "Root Terminal",
        description: "Run commands in your user directory",
    }
}

pub(in crate::server) fn public_root_desktop_link(
    config: &crate::config::DesktopConfig,
) -> Option<PublicRootDesktopLink> {
    config.enabled.then(|| PublicRootDesktopLink {
        href: format!("/{DESKTOP_ROUTE_SEGMENT}"),
        api_href: PUBLIC_API_ROOT_DESKTOP_PATH.to_string(),
        label: config.label.clone(),
        description: "View the desktop over VNC",
        view_only: config.view_only,
        screens: detect_desktop_screens(),
    })
}

pub(in crate::server) fn public_project_detail(
    project: &ProjectConfig,
    device_hostname: &str,
) -> PublicProjectDetail {
    let deployments = project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .map(public_deployment_summary(project))
        .collect::<Vec<_>>();

    PublicProjectDetail {
        name: project.name.clone(),
        device_hostname: device_hostname.to_string(),
        href: format!("/{}", project.name),
        api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}", project.name),
        summary: project_summary(project),
        deployment_count: deployments.len(),
        diff: PublicProjectDiffLink {
            href: format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT),
            api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}/diff", project.name),
            label: "Code changes",
            description: "Review staged and unstaged files",
        },
        terminal: PublicProjectTerminalLink {
            href: format!("/{}/{}", project.name, TERMINAL_ROUTE_SEGMENT),
            api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}/terminal", project.name),
            label: "Terminal",
            description: "Run commands in the project directory",
        },
        deployments,
    }
}

fn public_deployment_summary(
    project: &ProjectConfig,
) -> impl Fn(&ApplicationConfig) -> PublicDeploymentSummary + '_ {
    |deployment| PublicDeploymentSummary {
        name: deployment.name.clone(),
        href: format!("/{}/{}", project.name, deployment.name),
        kind: deployment_kind(deployment),
        label: deployment_home_label(deployment),
        media_type: deployment_media_type(deployment),
        title: deployment_page_title(deployment).map(str::to_string),
    }
}
