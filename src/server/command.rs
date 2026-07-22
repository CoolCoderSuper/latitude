use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, State},
    http::{Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    config::{ApplicationConfig, BootConfig, DeploymentShareConfig, current_unix_timestamp},
    state::AppState,
    storage::PageContent,
};

use super::{
    constants::{MAX_PAGE_PAYLOAD_BYTES, PUBLIC_SHARE_BASE_PATH},
    page::parse_page_payload,
    response::{ApiError, internal_response},
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
pub(in crate::server) struct CreateDeploymentShareRequest {
    pub(in crate::server) project: String,
    pub(in crate::server) deployment: String,
    #[serde(default)]
    pub(in crate::server) password: Option<String>,
    #[serde(default)]
    pub(in crate::server) expires_at: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(in crate::server) struct DeploymentShareResponse {
    token: String,
    project: String,
    deployment: String,
    href: String,
    has_password: bool,
    expires_at: Option<u64>,
    expired: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct T3CodeEmbedSessionRequest {
    pub(in crate::server) project: String,
    pub(in crate::server) theme: String,
}

#[derive(Debug, Serialize)]
pub(super) struct T3CodeEmbedSessionResponse {
    pub(in crate::server) href: String,
}

pub(super) async fn create_t3code_embed_session(
    State(state): State<AppState>,
    Json(request): Json<T3CodeEmbedSessionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let theme = match request.theme.as_str() {
        "light" => "light",
        "dark" => "dark",
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "theme must be 'light' or 'dark'",
            ));
        }
    };
    if state
        .catalog()
        .get_project(&request.project)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found(format!(
            "project '{}' was not found",
            request.project
        )));
    }
    let config = state.config_snapshot().await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let next = format!("/{}", request.project);
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", &token)
        .append_pair("next", &next)
        .append_pair("theme", theme)
        .finish();
    Ok(Json(T3CodeEmbedSessionResponse {
        href: format!("/__latitude/t3code/embed?{query}"),
    }))
}

pub(super) async fn command_health(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    let counts = state.catalog().counts().await?;
    Ok(Json(HealthResponse {
        status: "ok",
        public_bind: config.public_bind,
        command_bind: config.command_bind,
        project_count: counts.project_count,
        deployment_count: counts.deployment_count,
        share_link_count: counts.share_link_count,
    }))
}

pub(super) async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.config_snapshot().await)
}

pub(super) async fn put_config(
    State(state): State<AppState>,
    Json(config): Json<BootConfig>,
) -> Result<impl IntoResponse, ApiError> {
    config.validate()?;
    state.replace_config(config.clone()).await?;
    Ok(Json(config))
}

pub(super) async fn list_projects(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.catalog().list_projects().await?))
}

pub(super) async fn create_project(
    State(state): State<AppState>,
    Json(project): Json<crate::config::ProjectConfig>,
) -> Result<impl IntoResponse, ApiError> {
    project.validate()?;
    if let Some(discovered) = super::git::discover_worktree_project(&state, &project.project_dir)
        .await
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error))?
    {
        return Ok((StatusCode::OK, Json(discovered)));
    }
    let created = project.clone();
    state.catalog().create_project(project).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub(super) async fn get_project(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .catalog()
        .get_project(&project)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("project '{project}' was not found")))
}

pub(super) async fn replace_project(
    AxumPath(name): AxumPath<String>,
    State(state): State<AppState>,
    Json(mut project): Json<crate::config::ProjectConfig>,
) -> Result<impl IntoResponse, ApiError> {
    if project.name != name {
        project.name = name.clone();
    }
    project.validate()?;
    let replacement = project.clone();
    state.catalog().replace_project(project).await?;
    Ok(Json(replacement))
}

pub(super) async fn delete_project(
    AxumPath(name): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    if state.catalog().delete_project(&name).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("project was not found"))
    }
}

pub(super) async fn list_project_deployments(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    if state.catalog().get_project(&project).await?.is_none() {
        return Err(ApiError::not_found(format!(
            "project '{project}' was not found"
        )));
    }
    Ok(Json(
        state.catalog().list_project_deployments(&project).await?,
    ))
}

pub(super) async fn create_project_deployment(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    Json(app): Json<ApplicationConfig>,
) -> Result<impl IntoResponse, ApiError> {
    app.validate()?;
    let created = app.clone();
    state.catalog().create_deployment(&project, app).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub(super) async fn get_project_deployment(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .catalog()
        .get_deployment(&project, &name)
        .await?
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
    state.catalog().replace_deployment(&project, app).await?;
    let replacement = state
        .catalog()
        .get_deployment(&project, &name)
        .await?
        .ok_or_else(|| ApiError::not_found("deployment was not stored"))?;
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
    let bytes = page.payload_bytes()?;
    let deployment = state
        .catalog()
        .upsert_page(
            &project,
            &name,
            page.format,
            page.media_type,
            page.title,
            bytes,
        )
        .await?;

    Ok(Json(deployment))
}

pub(super) async fn get_project_page_content(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Result<Response<Body>, ApiError> {
    let content = state
        .catalog()
        .get_page_content(&project, &name)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "page deployment '{name}' was not found in project '{project}'"
            ))
        })?;
    Ok(page_content_response(content))
}

pub(super) async fn delete_project_deployment(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    if state.catalog().delete_deployment(&project, &name).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "deployment '{name}' was not found in project '{project}'"
        )))
    }
}

pub(super) async fn list_deployment_shares(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let now = current_unix_timestamp();
    let shares = state
        .catalog()
        .list_shares()
        .await?
        .iter()
        .map(|share| deployment_share_response(share, now))
        .collect::<Vec<_>>();

    Ok(Json(shares))
}

pub(super) async fn create_deployment_share(
    State(state): State<AppState>,
    Json(payload): Json<CreateDeploymentShareRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let share = state
        .catalog()
        .create_share(
            &payload.project,
            &payload.deployment,
            payload.password,
            payload.expires_at,
        )
        .await?;
    let now = current_unix_timestamp();

    Ok((
        StatusCode::CREATED,
        Json(deployment_share_response(&share, now)),
    ))
}

pub(super) async fn get_deployment_share(
    AxumPath(token): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let now = current_unix_timestamp();
    state
        .catalog()
        .get_share(&token)
        .await?
        .map(|share| Json(deployment_share_response(&share, now)))
        .ok_or_else(|| ApiError::not_found(format!("share link '{token}' was not found")))
}

pub(super) async fn delete_deployment_share(
    AxumPath(token): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    if state.catalog().delete_share(&token).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "share link '{token}' was not found"
        )))
    }
}

pub(in crate::server) fn deployment_share_response(
    share: &DeploymentShareConfig,
    now: u64,
) -> DeploymentShareResponse {
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

fn page_content_response(content: PageContent) -> Response<Body> {
    let content_type = content
        .media_type
        .or_else(|| match content.format {
            crate::config::PageFormat::Html => Some("text/html; charset=utf-8".to_string()),
            crate::config::PageFormat::Markdown => Some("text/markdown; charset=utf-8".to_string()),
            crate::config::PageFormat::Binary => None,
        })
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, content.bytes.len())
        .body(Body::from(content.bytes))
        .unwrap_or_else(internal_response)
}
