use axum::{
    Json,
    body::Body,
    extract::{Form, Path as AxumPath, State},
    http::{Response, StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::{
    command_protocol::CreateDeploymentShareRequest,
    config::current_unix_timestamp,
    server::{
        command::deployment_share_response,
        render::render_share_dialog_shell,
        response::{ApiError, json_error},
    },
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub(in crate::server) struct ShareUiForm {
    #[serde(default)]
    pub(in crate::server) password: Option<String>,
    #[serde(default)]
    pub(in crate::server) expiry: Option<u64>,
}

pub(in crate::server) async fn public_ui_get_shares(
    AxumPath((project, deployment)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Response<Body> {
    render_share_ui_response(&state, &project, &deployment, None).await
}

pub(in crate::server) async fn public_ui_create_share(
    AxumPath((project, deployment)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    Form(payload): Form<ShareUiForm>,
) -> Response<Body> {
    if let Err(error) = enabled_deployment(&state, &project, &deployment).await {
        return render_share_ui_response(&state, &project, &deployment, Some((&error, true))).await;
    }

    let password = payload
        .password
        .map(|password| password.trim().to_string())
        .filter(|password| !password.is_empty());
    let expires_at = payload
        .expiry
        .filter(|seconds| *seconds > 0)
        .map(|seconds| current_unix_timestamp().saturating_add(seconds));
    let result = state
        .catalog()
        .create_share(&project, &deployment, password, expires_at)
        .await;

    match result {
        Ok(_) => {
            render_share_ui_response(
                &state,
                &project,
                &deployment,
                Some(("Share link created.", false)),
            )
            .await
        }
        Err(error) => {
            let message = error.to_string();
            render_share_ui_response(&state, &project, &deployment, Some((&message, true))).await
        }
    }
}

pub(in crate::server) async fn public_ui_delete_share(
    AxumPath((project, deployment, token)): AxumPath<(String, String, String)>,
    State(state): State<AppState>,
) -> Response<Body> {
    let result = match state.catalog().get_share(&token).await {
        Ok(Some(share)) if share.project == project && share.deployment == deployment => {
            state.catalog().delete_share(&token).await
        }
        Ok(_) => Ok(false),
        Err(error) => Err(error),
    };

    match result {
        Ok(true) => {
            render_share_ui_response(
                &state,
                &project,
                &deployment,
                Some(("Share link revoked.", false)),
            )
            .await
        }
        Ok(false) => {
            render_share_ui_response(
                &state,
                &project,
                &deployment,
                Some(("Share link was not found.", true)),
            )
            .await
        }
        Err(error) => {
            let message = error.to_string();
            render_share_ui_response(&state, &project, &deployment, Some((&message, true))).await
        }
    }
}

async fn enabled_deployment(
    state: &AppState,
    project: &str,
    deployment: &str,
) -> Result<(), String> {
    let project_config = state
        .catalog()
        .get_project(project)
        .await
        .map_err(|error| error.to_string())?
        .filter(|project| project.enabled)
        .ok_or_else(|| format!("project '{project}' was not found"))?;

    project_config
        .deployments
        .iter()
        .any(|candidate| candidate.enabled && candidate.name == deployment)
        .then_some(())
        .ok_or_else(|| format!("deployment '{deployment}' was not found"))
}

async fn render_share_ui_response(
    state: &AppState,
    project: &str,
    deployment: &str,
    status: Option<(&str, bool)>,
) -> Response<Body> {
    let shares = match state.catalog().list_shares().await {
        Ok(shares) => shares,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let html = render_share_dialog_shell(project, deployment, &shares, status).into_string();
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

pub(in crate::server) async fn public_api_list_shares(
    State(state): State<AppState>,
) -> Response<Body> {
    match state.catalog().list_shares().await {
        Ok(shares) => {
            let now = current_unix_timestamp();
            Json(
                shares
                    .iter()
                    .map(|share| deployment_share_response(share, now))
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub(in crate::server) async fn public_api_create_share(
    State(state): State<AppState>,
    Json(payload): Json<CreateDeploymentShareRequest>,
) -> Response<Body> {
    match state
        .catalog()
        .create_share(
            &payload.project,
            &payload.deployment,
            payload.password,
            payload.expires_at,
        )
        .await
    {
        Ok(share) => (
            StatusCode::CREATED,
            Json(deployment_share_response(&share, current_unix_timestamp())),
        )
            .into_response(),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub(in crate::server) async fn public_api_delete_share(
    AxumPath(token): AxumPath<String>,
    State(state): State<AppState>,
) -> Response<Body> {
    match state.catalog().delete_share(&token).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => json_error(
            StatusCode::NOT_FOUND,
            format!("share link '{token}' was not found"),
        ),
        Err(error) => ApiError::from(error).into_response(),
    }
}
