use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, Query, State},
    http::{Request, Response, StatusCode, header},
    response::IntoResponse,
};
use maud::html;
use serde::Deserialize;

use super::{render::highlight_source_lines, response::json_error};
use crate::{
    config::ProjectConfig,
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
) -> Response<Body> {
    let project = match enabled_project(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };
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
    if let Some(bridge) = state.workspace_bridge() {
        return match bridge.proxy_file_get(request).await {
            Ok(response) => response,
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }
    state.project_files().get(request).await
}

pub(in crate::server) async fn public_api_put_project_file(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let project = match enabled_project(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };
    let body = match to_bytes(req.into_body(), MAX_FILE_EDITOR_BYTES + 4096).await {
        Ok(body) => body,
        Err(_) => return json_error(StatusCode::PAYLOAD_TOO_LARGE, "file is too large to save"),
    };
    let payload: SavePayload = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let request = ProjectFileWriteRequest {
        project_dir: project.project_dir,
        path: payload.path,
        content: payload.content,
    };
    if let Some(bridge) = state.workspace_bridge() {
        return match bridge.write_file(request).await {
            Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
            Err(error) => json_error(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
    }
    match state.project_files().write(request).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(error) => error.into_response(),
    }
}

pub(in crate::server) async fn public_ui_put_project_file(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let project = match enabled_project(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
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
    if let Some(bridge) = state.workspace_bridge() {
        return match bridge.write_file(request).await {
            Ok(()) => file_save_fragment("Saved", false),
            Err(error) => file_save_fragment(&error.to_string(), true),
        };
    }
    match state.project_files().write(request).await {
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
) -> Response<Body> {
    let project = match enabled_project(&state, &project).await {
        Ok(project) => project,
        Err(response) => return response,
    };
    let body = match to_bytes(req.into_body(), MAX_FILE_EDITOR_BYTES + 4096).await {
        Ok(body) => body,
        Err(_) => {
            return json_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "file is too large to highlight",
            );
        }
    };
    let payload: HighlightPayload = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error.to_string()),
    };
    if payload.content.len() > MAX_FILE_EDITOR_BYTES {
        return json_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "file is too large to highlight",
        );
    }
    if let Err(error) = resolve_project_target(&project.project_dir, &payload.path).await {
        return error.into_response();
    }

    Json(highlight_source_lines(&payload.content, &payload.path)).into_response()
}

async fn enabled_project(state: &AppState, name: &str) -> Result<ProjectConfig, Response<Body>> {
    match state.catalog().get_project(name).await {
        Ok(Some(project)) if project.enabled => Ok(project),
        Ok(_) => Err(json_error(StatusCode::NOT_FOUND, "project was not found")),
        Err(error) => Err(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )),
    }
}
