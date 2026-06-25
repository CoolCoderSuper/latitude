use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, State},
    http::{Request, StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{
    config::{
        ApplicationConfig, ApplicationTarget, ConfigError, DeploymentShareConfig, LatitudeConfig,
        ProjectConfig, current_unix_timestamp,
    },
    state::AppState,
};

use super::{
    constants::{MAX_PAGE_PAYLOAD_BYTES, PUBLIC_SHARE_BASE_PATH},
    page::parse_page_payload,
    response::ApiError,
};

#[derive(Debug, Serialize)]
pub(super) struct HealthResponse {
    status: &'static str,
    public_bind: String,
    command_bind: String,
    project_count: usize,
    deployment_count: usize,
    share_link_count: usize,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateDeploymentShareRequest {
    project: String,
    deployment: String,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    expires_at: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(super) struct DeploymentShareResponse {
    token: String,
    project: String,
    deployment: String,
    href: String,
    has_password: bool,
    expires_at: Option<u64>,
    expired: bool,
}

fn find_project_mut<'a>(
    config: &'a mut LatitudeConfig,
    name: &str,
) -> Result<&'a mut ProjectConfig, ApiError> {
    config
        .projects
        .iter_mut()
        .find(|project| project.name == name)
        .ok_or_else(|| ApiError::not_found(format!("project '{name}' was not found")))
}

pub(super) async fn command_health(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config_snapshot().await;
    let deployment_count = config
        .projects
        .iter()
        .map(|project| project.deployments.len())
        .sum();
    Json(HealthResponse {
        status: "ok",
        public_bind: config.public_bind,
        command_bind: config.command_bind,
        project_count: config.projects.len(),
        deployment_count,
        share_link_count: config.share_links.len(),
    })
}

pub(super) async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.config_snapshot().await)
}

pub(super) async fn put_config(
    State(state): State<AppState>,
    Json(config): Json<LatitudeConfig>,
) -> Result<impl IntoResponse, ApiError> {
    state.replace_config(config.clone()).await?;
    Ok(Json(config))
}

pub(super) async fn list_projects(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config_snapshot().await;
    Json(config.projects)
}

pub(super) async fn create_project(
    State(state): State<AppState>,
    Json(project): Json<ProjectConfig>,
) -> Result<impl IntoResponse, ApiError> {
    project.validate()?;
    let created = project.clone();

    state
        .update_config(|config| -> Result<(), ConfigError> {
            if config.projects.iter().any(|item| item.name == project.name) {
                return Err(ConfigError::Invalid(format!(
                    "project '{}' already exists",
                    project.name
                )));
            }
            config.projects.push(project);
            Ok(())
        })
        .await??;

    Ok((StatusCode::CREATED, Json(created)))
}

pub(super) async fn get_project(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    config
        .projects
        .into_iter()
        .find(|item| item.name == project)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("project '{project}' was not found")))
}

pub(super) async fn replace_project(
    AxumPath(name): AxumPath<String>,
    State(state): State<AppState>,
    Json(mut project): Json<ProjectConfig>,
) -> Result<impl IntoResponse, ApiError> {
    if project.name != name {
        project.name = name.clone();
    }
    project.validate()?;
    let replacement = project.clone();

    state
        .update_config(|config| -> Result<(), ConfigError> {
            if let Some(existing) = config
                .projects
                .iter_mut()
                .find(|existing| existing.name == name)
            {
                *existing = project;
                Ok(())
            } else {
                config.projects.push(project);
                Ok(())
            }
        })
        .await??;

    Ok(Json(replacement))
}

pub(super) async fn delete_project(
    AxumPath(name): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let removed = state
        .update_config(|config| {
            let before = config.projects.len();
            config.projects.retain(|project| project.name != name);
            let removed = before != config.projects.len();
            if removed {
                config.share_links.retain(|share| share.project != name);
            }
            removed
        })
        .await?;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("project was not found"))
    }
}

pub(super) async fn list_project_deployments(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    config
        .projects
        .into_iter()
        .find(|item| item.name == project)
        .map(|project| Json(project.deployments))
        .ok_or_else(|| ApiError::not_found(format!("project '{project}' was not found")))
}

pub(super) async fn create_project_deployment(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    Json(app): Json<ApplicationConfig>,
) -> Result<impl IntoResponse, ApiError> {
    app.validate()?;
    let created = app.clone();

    state
        .update_config_fallible(|config| -> Result<(), ApiError> {
            let project_config = find_project_mut(config, &project)?;
            if project_config
                .deployments
                .iter()
                .any(|item| item.name == app.name)
            {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "deployment '{}' already exists in project '{}'",
                        app.name, project
                    ),
                ));
            }
            project_config.deployments.push(app);
            Ok(())
        })
        .await??;

    Ok((StatusCode::CREATED, Json(created)))
}

pub(super) async fn get_project_deployment(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    let project_config = config
        .projects
        .into_iter()
        .find(|item| item.name == project)
        .ok_or_else(|| ApiError::not_found(format!("project '{project}' was not found")))?;

    project_config
        .deployments
        .into_iter()
        .find(|app| app.name == name)
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "deployment '{name}' was not found in project '{project}'"
            ))
        })
}

pub(super) async fn replace_project_deployment(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    Json(mut app): Json<ApplicationConfig>,
) -> Result<impl IntoResponse, ApiError> {
    if app.name != name {
        app.name = name.clone();
    }
    app.validate()?;
    let replacement = app.clone();

    state
        .update_config_fallible(|config| -> Result<(), ApiError> {
            let project_config = find_project_mut(config, &project)?;
            if let Some(existing) = project_config
                .deployments
                .iter_mut()
                .find(|existing| existing.name == name)
            {
                *existing = app;
            } else {
                project_config.deployments.push(app);
            }
            Ok(())
        })
        .await??;

    Ok(Json(replacement))
}

pub(super) async fn upsert_project_page(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<impl IntoResponse, ApiError> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (_parts, body) = req.into_parts();
    let body = to_bytes(body, MAX_PAGE_PAYLOAD_BYTES)
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("page payload could not be read: {error}"),
            )
        })?;

    let page = parse_page_payload(content_type.as_deref(), &body)?;
    let app = ApplicationConfig {
        name: name.clone(),
        enabled: true,
        target: ApplicationTarget::Page {
            content: page.content,
            format: page.format,
            media_type: page.media_type,
            title: page.title,
        },
    };
    app.validate()?;
    let replacement = app.clone();

    state
        .update_config_fallible(|config| -> Result<(), ApiError> {
            let project_config = find_project_mut(config, &project)?;
            if let Some(existing) = project_config
                .deployments
                .iter_mut()
                .find(|existing| existing.name == name)
            {
                *existing = app;
            } else {
                project_config.deployments.push(app);
            }
            Ok(())
        })
        .await??;

    Ok(Json(replacement))
}

pub(super) async fn delete_project_deployment(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let removed = state
        .update_config_fallible(|config| -> Result<bool, ApiError> {
            let project_config = find_project_mut(config, &project)?;
            let before = project_config.deployments.len();
            project_config.deployments.retain(|app| app.name != name);
            let removed = before != project_config.deployments.len();
            if removed {
                config
                    .share_links
                    .retain(|share| share.project != project || share.deployment != name);
            }
            Ok(removed)
        })
        .await??;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "deployment '{name}' was not found in project '{project}'"
        )))
    }
}

pub(super) async fn list_deployment_shares(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config_snapshot().await;
    let now = current_unix_timestamp();
    let shares = config
        .share_links
        .iter()
        .map(|share| deployment_share_response(share, now))
        .collect::<Vec<_>>();

    Json(shares)
}

pub(super) async fn create_deployment_share(
    State(state): State<AppState>,
    Json(payload): Json<CreateDeploymentShareRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let now = current_unix_timestamp();
    let share = state
        .update_config_fallible(|config| -> Result<DeploymentShareConfig, ApiError> {
            let Some(project) = config
                .projects
                .iter()
                .find(|project| project.name == payload.project)
            else {
                return Err(ApiError::not_found(format!(
                    "project '{}' was not found",
                    payload.project
                )));
            };

            if !project
                .deployments
                .iter()
                .any(|deployment| deployment.name == payload.deployment)
            {
                return Err(ApiError::not_found(format!(
                    "deployment '{}' was not found in project '{}'",
                    payload.deployment, payload.project
                )));
            }

            let password = payload
                .password
                .clone()
                .filter(|password| !password.is_empty());
            let share = DeploymentShareConfig {
                token: generate_share_token(config),
                project: payload.project.clone(),
                deployment: payload.deployment.clone(),
                password,
                expires_at: payload.expires_at,
            };
            share.validate()?;
            config.share_links.push(share.clone());
            Ok(share)
        })
        .await??;

    Ok((
        StatusCode::CREATED,
        Json(deployment_share_response(&share, now)),
    ))
}

pub(super) async fn get_deployment_share(
    AxumPath(token): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    let now = current_unix_timestamp();
    config
        .share_links
        .iter()
        .find(|share| share.token == token)
        .map(|share| Json(deployment_share_response(share, now)))
        .ok_or_else(|| ApiError::not_found(format!("share link '{token}' was not found")))
}

pub(super) async fn delete_deployment_share(
    AxumPath(token): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let removed = state
        .update_config(|config| {
            let before = config.share_links.len();
            config.share_links.retain(|share| share.token != token);
            before != config.share_links.len()
        })
        .await?;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "share link '{token}' was not found"
        )))
    }
}

fn deployment_share_response(share: &DeploymentShareConfig, now: u64) -> DeploymentShareResponse {
    DeploymentShareResponse {
        token: share.token.clone(),
        project: share.project.clone(),
        deployment: share.deployment.clone(),
        href: format!("{PUBLIC_SHARE_BASE_PATH}/{}/", share.token),
        has_password: share.password.is_some(),
        expires_at: share.expires_at,
        expired: share.is_expired(now),
    }
}

fn generate_share_token(config: &LatitudeConfig) -> String {
    loop {
        let token = encode_hex(rand::random::<[u8; 16]>());
        if !config.share_links.iter().any(|share| share.token == token) {
            return token;
        }
    }
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
