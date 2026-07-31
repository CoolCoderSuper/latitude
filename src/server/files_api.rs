use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, Query, State},
    http::{Request, Response, StatusCode, header},
    response::IntoResponse,
};
use maud::html;
use serde::Deserialize;

use super::{public::enabled_project, render::highlight_source_lines, response::ApiError};
use crate::{
    project_files::{
        MAX_FILE_EDITOR_BYTES, ProjectFileRequest, ProjectFileWriteRequest, resolve_project_target,
    },
    state::AppState,
};

#[derive(Deserialize)]
pub(super) struct FileQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    raw: bool,
    #[serde(default)]
    search: String,
    #[serde(default)]
    search_kind: String,
}

#[derive(Deserialize)]
struct SavePayload {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct HighlightPayload {
    path: String,
    content: String,
}

pub(in crate::server) async fn public_api_get_project_files(
    AxumPath(project): AxumPath<String>,
    Query(query): Query<FileQuery>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    let project = enabled_project(&state, &project).await?;
    let request = ProjectFileRequest {
        project_dir: project.project_dir,
        path: query.path,
        raw: query.raw,
        search: query.search,
        search_kind: query.search_kind,
        range: req
            .headers()
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
    };
    Ok(state.workspace().file_get(request).await?)
}

pub(in crate::server) async fn public_api_put_project_file(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<impl IntoResponse, ApiError> {
    let project = enabled_project(&state, &project).await?;
    let body = to_bytes(req.into_body(), MAX_FILE_EDITOR_BYTES + 4096)
        .await
        .map_err(|_| ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "file is too large to save"))?;
    let payload: SavePayload = serde_json::from_slice(&body)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    let request = ProjectFileWriteRequest {
        project_dir: project.project_dir,
        path: payload.path,
        content: payload.content,
    };
    state.workspace().write_file(request).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(in crate::server) async fn public_ui_put_project_file(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let project = match enabled_project(&state, &project).await {
        Ok(project) => project,
        Err(error) => return error.into_response(),
    };
    let body = match to_bytes(req.into_body(), MAX_FILE_EDITOR_BYTES * 3 + 4096).await {
        Ok(body) => body,
        Err(_) => return file_save_fragment("File is too large to save.", true),
    };
    let mut path = None;
    let mut content = None;
    for (name, value) in url::form_urlencoded::parse(&body) {
        match name.as_ref() {
            "path" => path = Some(value.into_owned()),
            "content" => content = Some(value.into_owned()),
            _ => {}
        }
    }
    let Some(path) = path.filter(|path| !path.is_empty()) else {
        return file_save_fragment("File path is required.", true);
    };
    let Some(content) = content else {
        return file_save_fragment("File content is required.", true);
    };
    let request = ProjectFileWriteRequest {
        project_dir: project.project_dir,
        path,
        content,
    };
    match state.workspace().write_file(request).await {
        Ok(()) => file_save_fragment("Saved", false),
        Err(error) => file_save_fragment(&error.message, true),
    }
}

fn file_save_fragment(message: &str, is_error: bool) -> Response<Body> {
    let markup = html! {
        span data-file-save-result data-ok=(!is_error) { (message) }
    };
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        markup.into_string(),
    )
        .into_response()
}

pub(in crate::server) async fn public_api_highlight_project_file(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<impl IntoResponse, ApiError> {
    let project = enabled_project(&state, &project).await?;
    let body = to_bytes(req.into_body(), MAX_FILE_EDITOR_BYTES + 4096)
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "file is too large to highlight",
            )
        })?;
    let payload: HighlightPayload = serde_json::from_slice(&body)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    if payload.content.len() > MAX_FILE_EDITOR_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "file is too large to highlight",
        ));
    }
    resolve_project_target(&project.project_dir, &payload.path).await?;

    Ok(Json(highlight_source_lines(
        &payload.content,
        &payload.path,
    )))
}
