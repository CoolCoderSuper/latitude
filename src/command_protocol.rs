use serde::{Deserialize, Serialize};

pub(crate) const HEALTH_PATH: &str = "/health";
pub(crate) const CONFIG_PATH: &str = "/api/config";
pub(crate) const PROJECTS_PATH: &str = "/api/projects";
pub(crate) const PROJECT_PATH: &str = "/api/projects/{project}";
pub(crate) const PROJECT_DEPLOYMENTS_PATH: &str = "/api/projects/{project}/deployments";
pub(crate) const PROJECT_DEPLOYMENT_PATH: &str = "/api/projects/{project}/deployments/{name}";
pub(crate) const PROJECT_DEPLOYMENT_ARCHIVE_PATH: &str =
    "/api/projects/{project}/deployments/{name}/archive";
pub(crate) const PROJECT_PAGE_PATH: &str = "/api/projects/{project}/pages/{name}";
pub(crate) const PROJECT_PAGE_CONTENT_PATH: &str = "/api/projects/{project}/pages/{name}/content";
pub(crate) const SHARES_PATH: &str = "/api/shares";
pub(crate) const SHARE_PATH: &str = "/api/shares/{token}";
pub(crate) const T3CODE_EMBED_SESSION_PATH: &str = "/api/t3code/embed-session";

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: String,
    pub(crate) public_bind: String,
    pub(crate) command_bind: String,
    pub(crate) project_count: usize,
    pub(crate) deployment_count: usize,
    pub(crate) share_link_count: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CreateDeploymentShareRequest {
    pub(crate) project: String,
    pub(crate) deployment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct DeploymentShareResponse {
    pub(crate) token: String,
    pub(crate) project: String,
    pub(crate) deployment: String,
    pub(crate) href: String,
    pub(crate) has_password: bool,
    pub(crate) expires_at: Option<u64>,
    pub(crate) expired: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct DeploymentArchiveRequest {
    pub(crate) archived: bool,
}

pub(crate) fn project_path(project: &str) -> String {
    format!("/api/projects/{project}")
}

pub(crate) fn project_deployments_path(project: &str) -> String {
    format!("/api/projects/{project}/deployments")
}

pub(crate) fn project_deployment_path(project: &str, deployment: &str) -> String {
    format!("/api/projects/{project}/deployments/{deployment}")
}

pub(crate) fn project_deployment_archive_path(project: &str, deployment: &str) -> String {
    format!("/api/projects/{project}/deployments/{deployment}/archive")
}

pub(crate) fn project_page_path(project: &str, page: &str) -> String {
    format!("/api/projects/{project}/pages/{page}")
}

pub(crate) fn share_path(token: &str) -> String {
    format!("/api/shares/{token}")
}
