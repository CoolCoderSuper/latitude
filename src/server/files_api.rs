use std::{
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, Query, State},
    http::{Request, Response, StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::fs;

use super::{
    auth::{public_api_auth_challenge, public_request_is_authenticated},
    constants::MAX_FILE_EDITOR_BYTES,
    git::file_baseline,
    render::highlight_source_lines,
    response::json_error,
};
use crate::{config::ProjectConfig, state::AppState};

#[derive(Deserialize)]
pub(super) struct FileQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    raw: bool,
}

#[derive(Serialize)]
struct FileEntry {
    name: String,
    path: String,
    kind: &'static str,
    size: u64,
}

#[derive(Serialize)]
struct DirectoryResponse {
    path: String,
    entries: Vec<FileEntry>,
}

#[derive(Serialize)]
struct FileResponse {
    path: String,
    name: String,
    content: String,
    media_type: String,
    editable: bool,
    size: u64,
    modified: Option<u64>,
    git_base_content: Option<String>,
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
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }
    let project = match enabled_project(&state, &project).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let (root, target) = match safe_target(&project, &query.path).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let metadata = match fs::metadata(&target).await {
        Ok(m) => m,
        Err(_) => return json_error(StatusCode::NOT_FOUND, "file was not found"),
    };
    if metadata.is_dir() {
        let mut reader = match fs::read_dir(&target).await {
            Ok(r) => r,
            Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        let mut entries = Vec::new();
        while let Ok(Some(entry)) = reader.next_entry().await {
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            let Ok(canonical) = fs::canonicalize(entry.path()).await else {
                continue;
            };
            if !canonical.starts_with(&root) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = canonical
                .strip_prefix(&root)
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .replace('\\', "/");
            entries.push(FileEntry {
                name,
                path: rel,
                kind: if meta.is_dir() { "directory" } else { "file" },
                size: meta.len(),
            });
        }
        entries.sort_by(|a, b| {
            (a.kind != "directory", a.name.to_lowercase())
                .cmp(&(b.kind != "directory", b.name.to_lowercase()))
        });
        return Json(DirectoryResponse {
            path: query.path,
            entries,
        })
        .into_response();
    }
    let media_type = mime_guess::from_path(&target)
        .first_or_octet_stream()
        .to_string();
    let bytes = match fs::read(&target).await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if query.raw {
        return Response::builder()
            .header(header::CONTENT_TYPE, media_type)
            .header(header::CONTENT_LENGTH, bytes.len())
            .body(Body::from(bytes))
            .unwrap();
    }
    let content = match String::from_utf8(bytes) { Ok(s) => s, Err(_) => return Json(serde_json::json!({"path": query.path, "name": target.file_name().unwrap_or_default().to_string_lossy(), "media_type": media_type, "editable": false, "size": metadata.len(), "binary": true})).into_response() };
    let git_base_content = if metadata.len() <= MAX_FILE_EDITOR_BYTES as u64 {
        file_baseline(&project.project_dir, &target).await
    } else {
        None
    };
    Json(FileResponse {
        path: query.path,
        name: target
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        content,
        media_type,
        editable: metadata.len() <= MAX_FILE_EDITOR_BYTES as u64,
        size: metadata.len(),
        modified: metadata
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        git_base_content,
    })
    .into_response()
}

pub(in crate::server) async fn public_api_put_project_file(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }
    let project = match enabled_project(&state, &project).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let body = match to_bytes(req.into_body(), MAX_FILE_EDITOR_BYTES + 4096).await {
        Ok(b) => b,
        Err(_) => return json_error(StatusCode::PAYLOAD_TOO_LARGE, "file is too large to save"),
    };
    let payload: SavePayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e.to_string()),
    };
    if payload.content.len() > MAX_FILE_EDITOR_BYTES {
        return json_error(StatusCode::PAYLOAD_TOO_LARGE, "file is too large to save");
    }
    let (_, target) = match safe_target(&project, &payload.path).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if !fs::metadata(&target).await.is_ok_and(|m| m.is_file()) {
        return json_error(StatusCode::BAD_REQUEST, "only existing files can be edited");
    }
    match fs::write(&target, payload.content).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub(in crate::server) async fn public_api_highlight_project_file(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }
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
    if let Err(response) = safe_target(&project, &payload.path).await {
        return response;
    }

    Json(highlight_source_lines(&payload.content, &payload.path)).into_response()
}

async fn enabled_project(state: &AppState, name: &str) -> Result<ProjectConfig, Response<Body>> {
    match state.catalog().get_project(name).await {
        Ok(Some(p)) if p.enabled => Ok(p),
        Ok(_) => Err(json_error(StatusCode::NOT_FOUND, "project was not found")),
        Err(e) => Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn safe_target(
    project: &ProjectConfig,
    relative: &str,
) -> Result<(PathBuf, PathBuf), Response<Body>> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(json_error(StatusCode::BAD_REQUEST, "invalid file path"));
    }
    let root = fs::canonicalize(&project.project_dir)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let target = fs::canonicalize(root.join(path))
        .await
        .map_err(|_| json_error(StatusCode::NOT_FOUND, "file was not found"))?;
    if !target.starts_with(&root) {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "file is outside the project",
        ));
    }
    Ok((root, target))
}
