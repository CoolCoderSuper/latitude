use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, State},
    http::{Request, StatusCode, header},
    response::IntoResponse,
};
use serde::Serialize;

use crate::{
    config::{ApplicationConfig, ApplicationTarget, ConfigError, LatitudeConfig, ProjectConfig},
    state::AppState,
};

use super::{constants::MAX_PAGE_PAYLOAD_BYTES, page::parse_page_payload, response::ApiError};

#[derive(Debug, Serialize)]
pub(super) struct HealthResponse {
    status: &'static str,
    public_bind: String,
    command_bind: String,
    project_count: usize,
    deployment_count: usize,
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
            before != config.projects.len()
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
            Ok(before != project_config.deployments.len())
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
