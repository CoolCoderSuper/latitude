use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, Response, StatusCode, header},
    response::IntoResponse,
};
use fff_search::{
    FFFMode, FFFQuery, FilePicker, FilePickerOptions, FileSearchConfig, FuzzySearchOptions,
    GrepConfig, GrepMode, GrepSearchOptions, PaginationArgs, SharedFilePicker, SharedFrecency,
};
use serde::Serialize;
use tokio::fs;

use crate::server::file_baseline;

use super::{
    WorkspaceBridge, WorkspaceFileRequest, WorkspaceFileWriteRequest, WorkspaceHostState,
    host::{workspace_error, workspace_is_authenticated},
};

pub(super) const WORKSPACE_FILES_PATH: &str = "/files";
pub(super) const WORKSPACE_FILE_WRITE_PATH: &str = "/files/write";
const MAX_INTERNAL_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_EDITABLE_FILE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, Default)]
pub(super) struct WorkspaceFiles {
    pickers: Arc<Mutex<HashMap<PathBuf, SharedFilePicker>>>,
}

#[derive(Serialize)]
struct WorkspaceFileEntry {
    name: String,
    path: String,
    kind: &'static str,
    size: u64,
}

#[derive(Serialize)]
struct WorkspaceDirectoryResponse {
    path: String,
    entries: Vec<WorkspaceFileEntry>,
}

#[derive(Serialize)]
struct WorkspaceSearchResult {
    path: String,
    line: Option<usize>,
    column: Option<usize>,
    preview: Option<String>,
}

#[derive(Serialize)]
struct WorkspaceSearchResponse {
    results: Vec<WorkspaceSearchResult>,
    limited: bool,
}

#[derive(Serialize)]
struct WorkspaceFileResponse {
    path: String,
    name: String,
    content: String,
    media_type: String,
    editable: bool,
    size: u64,
    modified: Option<u64>,
    git_base_content: Option<String>,
}

impl WorkspaceBridge {
    pub(crate) async fn proxy_file_get(
        &self,
        request: WorkspaceFileRequest,
    ) -> Result<Response<Body>> {
        self.proxy_file_request(WORKSPACE_FILES_PATH, &request)
            .await
    }

    pub(crate) async fn write_file(&self, request: WorkspaceFileWriteRequest) -> Result<()> {
        let response = self
            .proxy_file_request(WORKSPACE_FILE_WRITE_PATH, &request)
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), MAX_INTERNAL_FILE_BYTES)
                .await
                .unwrap_or_default();
            return Err(anyhow!(
                "workspace file save failed ({status}): {}",
                String::from_utf8_lossy(&bytes)
            ));
        }
        Ok(())
    }

    async fn proxy_file_request<T: Serialize + ?Sized>(
        &self,
        path: &str,
        request: &T,
    ) -> Result<Response<Body>> {
        let endpoint = self.endpoint().await?;
        let url = format!("http://{}{}", endpoint.address, path);
        let response = reqwest::Client::new()
            .post(url)
            .bearer_auth(&endpoint.token)
            .json(request)
            .send()
            .await
            .context("workspace file host is unavailable")?;
        let status = StatusCode::from_u16(response.status().as_u16())
            .context("workspace file status was invalid")?;
        let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
        let bytes = response
            .bytes()
            .await
            .context("workspace file response could not be read")?;
        let mut builder = Response::builder().status(status);
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        builder
            .header(header::CONTENT_LENGTH, bytes.len())
            .body(Body::from(bytes))
            .context("workspace file response could not be built")
    }
}

pub(super) async fn workspace_files(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    Json(request): Json<WorkspaceFileRequest>,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !request.search.trim().is_empty() {
        return workspace_file_search(&state.files, request).await;
    }
    workspace_file_read(request).await
}

async fn workspace_file_search(
    files: &WorkspaceFiles,
    request: WorkspaceFileRequest,
) -> Response<Body> {
    let project_dir = request.project_dir;
    let needle = request.search.trim().to_string();
    let grep = request.search_kind == "grep";
    let pickers = files.pickers.clone();
    match tokio::task::spawn_blocking(move || {
        let picker = workspace_file_picker(&pickers, &project_dir)?;
        picker.wait_for_indexing_complete(Duration::from_secs(10));
        search_workspace_files(&picker, &needle, grep)
    })
    .await
    {
        Ok(Ok(response)) => Json(response).into_response(),
        Ok(Err(error)) => workspace_error(StatusCode::INTERNAL_SERVER_ERROR, error),
        Err(error) => workspace_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

fn workspace_file_picker(
    pickers: &Mutex<HashMap<PathBuf, SharedFilePicker>>,
    project_dir: &Path,
) -> Result<SharedFilePicker, String> {
    let project_dir = std::fs::canonicalize(project_dir).map_err(|error| error.to_string())?;
    let mut pickers = pickers
        .lock()
        .map_err(|_| "workspace file search index lock was poisoned".to_string())?;
    if let Some(picker) = pickers.get(&project_dir) {
        return Ok(picker.clone());
    }
    let picker = SharedFilePicker::default();
    FilePicker::new_with_shared_state(
        picker.clone(),
        SharedFrecency::default(),
        FilePickerOptions {
            base_path: project_dir.to_string_lossy().into_owned(),
            enable_mmap_cache: true,
            enable_content_indexing: true,
            mode: FFFMode::Ai,
            watch: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    pickers.insert(project_dir, picker.clone());
    Ok(picker)
}

fn search_workspace_files(
    shared_picker: &SharedFilePicker,
    needle: &str,
    grep: bool,
) -> Result<WorkspaceSearchResponse, String> {
    const MAX_RESULTS: usize = 100;
    const MAX_SEARCH_FILE_BYTES: u64 = 1024 * 1024;
    let guard = shared_picker.read().map_err(|error| error.to_string())?;
    let picker = guard
        .as_ref()
        .ok_or_else(|| "file search index is not ready".to_string())?;

    if !grep {
        let query = FFFQuery::parse(needle, FileSearchConfig);
        let found = picker.fuzzy_search(
            &query,
            None,
            FuzzySearchOptions {
                pagination: PaginationArgs {
                    offset: 0,
                    limit: MAX_RESULTS,
                },
                ..Default::default()
            },
        );
        let results = found
            .items
            .iter()
            .map(|file| WorkspaceSearchResult {
                path: file.relative_path(picker).replace('\\', "/"),
                line: None,
                column: None,
                preview: None,
            })
            .collect();
        return Ok(WorkspaceSearchResponse {
            results,
            limited: found.total_matched > found.items.len(),
        });
    }

    let query = FFFQuery::parse(needle, GrepConfig);
    let found = picker.grep(
        &query,
        &GrepSearchOptions {
            max_file_size: MAX_SEARCH_FILE_BYTES,
            max_matches_per_file: MAX_RESULTS,
            smart_case: true,
            page_limit: MAX_RESULTS,
            mode: GrepMode::PlainText,
            time_budget_ms: 2_000,
            ..Default::default()
        },
    );
    let limited = found.next_file_offset != 0 || found.matches.len() > MAX_RESULTS;
    let results = found
        .matches
        .iter()
        .take(MAX_RESULTS)
        .map(|matched| {
            let file = found.files[matched.file_index];
            WorkspaceSearchResult {
                path: file.relative_path(picker).replace('\\', "/"),
                line: Some(matched.line_number as usize),
                column: Some(matched.line_content[..matched.col].chars().count() + 1),
                preview: Some(matched.line_content.trim().chars().take(240).collect()),
            }
        })
        .collect();
    Ok(WorkspaceSearchResponse { results, limited })
}

async fn workspace_file_read(request: WorkspaceFileRequest) -> Response<Body> {
    let (root, target) = match safe_workspace_target(&request.project_dir, &request.path).await {
        Ok(target) => target,
        Err((status, error)) => return workspace_error(status, error),
    };
    let metadata = match fs::metadata(&target).await {
        Ok(metadata) => metadata,
        Err(_) => return workspace_error(StatusCode::NOT_FOUND, "file was not found"),
    };
    if metadata.is_dir() {
        let mut reader = match fs::read_dir(&target).await {
            Ok(reader) => reader,
            Err(error) => {
                return workspace_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
            }
        };
        let mut entries = Vec::new();
        while let Ok(Some(entry)) = reader.next_entry().await {
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            let Ok(canonical) = fs::canonicalize(entry.path()).await else {
                continue;
            };
            if !canonical.starts_with(&root) {
                continue;
            }
            entries.push(WorkspaceFileEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: canonical
                    .strip_prefix(&root)
                    .unwrap_or(Path::new(""))
                    .to_string_lossy()
                    .replace('\\', "/"),
                kind: if metadata.is_dir() {
                    "directory"
                } else {
                    "file"
                },
                size: metadata.len(),
            });
        }
        entries.sort_by(|left, right| {
            (left.kind != "directory", left.name.to_lowercase())
                .cmp(&(right.kind != "directory", right.name.to_lowercase()))
        });
        return Json(WorkspaceDirectoryResponse {
            path: request.path,
            entries,
        })
        .into_response();
    }

    let media_type = mime_guess::from_path(&target)
        .first_or_octet_stream()
        .to_string();
    let bytes = match fs::read(&target).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return workspace_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
        }
    };
    if request.raw {
        return Response::builder()
            .header(header::CONTENT_TYPE, media_type)
            .header(header::CONTENT_LENGTH, bytes.len())
            .body(Body::from(bytes))
            .unwrap_or_else(|error| {
                workspace_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            });
    }
    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(_) => {
            return Json(serde_json::json!({
                "path": request.path,
                "name": target.file_name().unwrap_or_default().to_string_lossy(),
                "media_type": media_type,
                "editable": false,
                "size": metadata.len(),
                "binary": true
            }))
            .into_response();
        }
    };
    let git_base_content = if metadata.len() <= MAX_EDITABLE_FILE_BYTES as u64 {
        file_baseline(&request.project_dir, &target).await
    } else {
        None
    };
    Json(WorkspaceFileResponse {
        path: request.path,
        name: target
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        content,
        media_type,
        editable: metadata.len() <= MAX_EDITABLE_FILE_BYTES as u64,
        size: metadata.len(),
        modified: metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs()),
        git_base_content,
    })
    .into_response()
}

pub(super) async fn workspace_file_write(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    Json(request): Json<WorkspaceFileWriteRequest>,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if request.content.len() > MAX_INTERNAL_FILE_BYTES {
        return workspace_error(StatusCode::PAYLOAD_TOO_LARGE, "file is too large to save");
    }
    let (_, target) = match safe_workspace_target(&request.project_dir, &request.path).await {
        Ok(target) => target,
        Err((status, error)) => return workspace_error(status, error),
    };
    if !fs::metadata(&target)
        .await
        .is_ok_and(|metadata| metadata.is_file())
    {
        return workspace_error(StatusCode::BAD_REQUEST, "only existing files can be edited");
    }
    match fs::write(target, request.content).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(error) => workspace_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn safe_workspace_target(
    project_dir: &Path,
    relative: &str,
) -> std::result::Result<(PathBuf, PathBuf), (StatusCode, String)> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err((StatusCode::BAD_REQUEST, "invalid file path".to_string()));
    }
    let root = fs::canonicalize(project_dir).await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("project directory could not be opened: {error}"),
        )
    })?;
    let target = fs::canonicalize(root.join(path))
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "file was not found".to_string()))?;
    if !target.starts_with(&root) {
        return Err((
            StatusCode::FORBIDDEN,
            "file is outside the project".to_string(),
        ));
    }
    Ok((root, target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn workspace_paths_reject_parent_traversal() {
        let result = safe_workspace_target(Path::new("C:/work/demo"), "../secret.txt").await;
        assert!(matches!(result, Err((StatusCode::BAD_REQUEST, _))));
    }
}
