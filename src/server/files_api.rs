use std::{
    path::{Component, Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, Query, State},
    http::{Request, Response, StatusCode, header},
    response::IntoResponse,
};
use fff_search::{
    FFFQuery, FileSearchConfig, FuzzySearchOptions, GrepConfig, GrepMode, GrepSearchOptions,
    PaginationArgs, SharedFilePicker,
};
use maud::html;
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
    #[serde(default)]
    search: String,
    #[serde(default)]
    search_kind: String,
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
struct SearchResult {
    path: String,
    line: Option<usize>,
    column: Option<usize>,
    preview: Option<String>,
}

#[derive(Serialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
    limited: bool,
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
    if !query.search.trim().is_empty() {
        let search_state = state.clone();
        let project_dir = project.project_dir.clone();
        let needle = query.search.trim().to_string();
        let kind = query.search_kind.clone();
        return match tokio::task::spawn_blocking(move || {
            let picker = search_state.file_search_picker(&project_dir)?;
            picker.wait_for_indexing_complete(Duration::from_secs(10));
            search_project_files(&picker, &needle, kind == "grep")
        })
        .await
        {
            Ok(Ok(response)) => Json(response).into_response(),
            Ok(Err(message)) => json_error(StatusCode::INTERNAL_SERVER_ERROR, message),
            Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };
    }
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

fn search_project_files(
    shared_picker: &SharedFilePicker,
    needle: &str,
    grep: bool,
) -> Result<SearchResponse, String> {
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
            .map(|file| SearchResult {
                path: file.relative_path(picker).replace('\\', "/"),
                line: None,
                column: None,
                preview: None,
            })
            .collect();
        return Ok(SearchResponse {
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
            let column = matched.line_content[..matched.col].chars().count() + 1;
            SearchResult {
                path: file.relative_path(picker).replace('\\', "/"),
                line: Some(matched.line_number as usize),
                column: Some(column),
                preview: Some(matched.line_content.trim().chars().take(240).collect()),
            }
        })
        .collect();
    Ok(SearchResponse { results, limited })
}

#[cfg(test)]
mod search_tests {
    use super::search_project_files;
    use fff_search::{FFFMode, FilePicker, FilePickerOptions, SharedFilePicker, SharedFrecency};

    #[test]
    fn finds_files_and_content_while_honoring_gitignore() {
        let root = std::env::temp_dir().join(format!(
            "latitude-file-search-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("ignored")).unwrap();
        std::fs::create_dir_all(root.join(".git/refs/heads")).unwrap();
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            root.join(".git/config"),
            "[core]\nrepositoryformatversion = 0\nbare = false\n",
        )
        .unwrap();
        std::fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::write(
            root.join("src/search_widget.rs"),
            "fn main() {\n    println!(\"Needle\");\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("ignored/search_widget.txt"), "needle").unwrap();

        let picker = SharedFilePicker::default();
        FilePicker::new_with_shared_state(
            picker.clone(),
            SharedFrecency::default(),
            FilePickerOptions {
                base_path: root.to_string_lossy().into_owned(),
                mode: FFFMode::Ai,
                watch: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(picker.wait_for_indexing_complete(std::time::Duration::from_secs(10)));

        let files = search_project_files(&picker, "widget", false).unwrap();
        assert_eq!(files.results.len(), 1);
        assert_eq!(files.results[0].path, "src/search_widget.rs");

        let matches = search_project_files(&picker, "needle", true).unwrap();
        assert_eq!(matches.results.len(), 1);
        assert_eq!(matches.results[0].path, "src/search_widget.rs");
        assert_eq!(matches.results[0].line, Some(2));
        assert_eq!(matches.results[0].column, Some(15));

        std::fs::remove_dir_all(root).unwrap();
    }
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

pub(in crate::server) async fn public_ui_put_project_file(
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
    if content.len() > MAX_FILE_EDITOR_BYTES {
        return file_save_fragment("File is too large to save.", true);
    }
    let (_, target) = match safe_target(&project, &path).await {
        Ok(target) => target,
        Err(_) => return file_save_fragment("File could not be opened safely.", true),
    };
    if !fs::metadata(&target)
        .await
        .is_ok_and(|metadata| metadata.is_file())
    {
        return file_save_fragment("Only existing files can be edited.", true);
    }

    match fs::write(&target, content).await {
        Ok(_) => file_save_fragment("Saved", false),
        Err(error) => file_save_fragment(&error.to_string(), true),
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
