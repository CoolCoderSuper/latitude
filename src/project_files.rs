use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, UNIX_EPOCH},
};

use axum::{
    Json,
    body::Body,
    http::{Method, Response, StatusCode},
    response::IntoResponse,
};
use fff_search::{
    FFFMode, FFFQuery, FilePicker, FilePickerOptions, FileSearchConfig, FuzzySearchOptions,
    GrepConfig, GrepMode, GrepSearchOptions, PaginationArgs, SharedFilePicker, SharedFrecency,
};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::{http_stream::file_response, server::file_baseline};

pub(crate) const MAX_FILE_EDITOR_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProjectFileRequest {
    pub(crate) project_dir: PathBuf,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) raw: bool,
    #[serde(default)]
    pub(crate) search: String,
    #[serde(default)]
    pub(crate) search_kind: String,
    #[serde(default)]
    pub(crate) range: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProjectFileWriteRequest {
    pub(crate) project_dir: PathBuf,
    pub(crate) path: String,
    pub(crate) content: String,
}

#[derive(Clone, Default)]
pub(crate) struct ProjectFileService {
    pickers: Arc<Mutex<HashMap<PathBuf, SharedFilePicker>>>,
}

#[derive(Debug)]
pub(crate) struct ProjectFileError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

#[derive(Serialize)]
struct ProjectFileEntry {
    name: String,
    path: String,
    kind: &'static str,
    size: u64,
}

#[derive(Serialize)]
struct ProjectDirectoryResponse {
    path: String,
    entries: Vec<ProjectFileEntry>,
}

#[derive(Serialize)]
struct ProjectSearchResult {
    path: String,
    line: Option<usize>,
    column: Option<usize>,
    preview: Option<String>,
}

#[derive(Serialize)]
struct ProjectSearchResponse {
    results: Vec<ProjectSearchResult>,
    limited: bool,
}

#[derive(Serialize)]
struct ProjectFileResponse {
    path: String,
    name: String,
    content: Option<String>,
    media_type: String,
    editable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    binary: Option<bool>,
    size: u64,
    modified: Option<u64>,
    git_base_content: Option<String>,
}

impl ProjectFileService {
    pub(crate) async fn get(&self, request: ProjectFileRequest) -> Response<Body> {
        if !request.search.trim().is_empty() {
            return self.search(request).await;
        }
        self.read(request).await
    }

    async fn search(&self, request: ProjectFileRequest) -> Response<Body> {
        let project_dir = request.project_dir;
        let needle = request.search.trim().to_string();
        let grep = request.search_kind == "grep";
        let pickers = self.pickers.clone();
        match tokio::task::spawn_blocking(move || {
            let picker = project_file_picker(&pickers, &project_dir)?;
            picker.wait_for_indexing_complete(Duration::from_secs(10));
            search_project_files(&picker, &needle, grep)
        })
        .await
        {
            Ok(Ok(response)) => Json(response).into_response(),
            Ok(Err(error)) => project_file_error(StatusCode::INTERNAL_SERVER_ERROR, error),
            Err(error) => project_file_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        }
    }

    async fn read(&self, request: ProjectFileRequest) -> Response<Body> {
        let (root, target) = match resolve_project_target(&request.project_dir, &request.path).await
        {
            Ok(target) => target,
            Err(error) => return error.into_response(),
        };
        let metadata = match fs::metadata(&target).await {
            Ok(metadata) => metadata,
            Err(_) => {
                return project_file_error(StatusCode::NOT_FOUND, "file was not found");
            }
        };
        if metadata.is_dir() {
            return read_directory(&root, &target, request.path).await;
        }

        let media_type = mime_guess::from_path(&target)
            .first_or_octet_stream()
            .to_string();
        if request.raw {
            return match file_response(&Method::GET, request.range.as_deref(), &target, &media_type)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    project_file_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
                }
            };
        }

        let editable_size = metadata.len() <= MAX_FILE_EDITOR_BYTES as u64;
        let content = if editable_size {
            match fs::read(&target).await {
                Ok(bytes) => String::from_utf8(bytes).ok(),
                Err(error) => {
                    return project_file_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        error.to_string(),
                    );
                }
            }
        } else {
            None
        };
        let editable = content.is_some();
        let git_base_content = if editable {
            file_baseline(&request.project_dir, &target).await
        } else {
            None
        };
        Json(ProjectFileResponse {
            path: request.path,
            name: target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            binary: editable_size.then_some(content.is_none()),
            content,
            media_type,
            editable,
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

    pub(crate) async fn write(
        &self,
        request: ProjectFileWriteRequest,
    ) -> Result<(), ProjectFileError> {
        if request.content.len() > MAX_FILE_EDITOR_BYTES {
            return Err(ProjectFileError {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                message: "file is too large to save".to_string(),
            });
        }
        let (_, target) = resolve_project_target(&request.project_dir, &request.path).await?;
        if !fs::metadata(&target)
            .await
            .is_ok_and(|metadata| metadata.is_file())
        {
            return Err(ProjectFileError {
                status: StatusCode::BAD_REQUEST,
                message: "only existing files can be edited".to_string(),
            });
        }
        fs::write(target, request.content)
            .await
            .map_err(|error| ProjectFileError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: error.to_string(),
            })
    }
}

impl ProjectFileError {
    pub(crate) fn into_response(self) -> Response<Body> {
        project_file_error(self.status, self.message)
    }
}

pub(crate) async fn resolve_project_target(
    project_dir: &Path,
    relative: &str,
) -> Result<(PathBuf, PathBuf), ProjectFileError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(ProjectFileError {
            status: StatusCode::BAD_REQUEST,
            message: "invalid file path".to_string(),
        });
    }
    let root = fs::canonicalize(project_dir)
        .await
        .map_err(|error| ProjectFileError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("project directory could not be opened: {error}"),
        })?;
    let target = fs::canonicalize(root.join(path))
        .await
        .map_err(|_| ProjectFileError {
            status: StatusCode::NOT_FOUND,
            message: "file was not found".to_string(),
        })?;
    if !target.starts_with(&root) {
        return Err(ProjectFileError {
            status: StatusCode::FORBIDDEN,
            message: "file is outside the project".to_string(),
        });
    }
    Ok((root, target))
}

async fn read_directory(root: &Path, target: &Path, path: String) -> Response<Body> {
    let mut reader = match fs::read_dir(target).await {
        Ok(reader) => reader,
        Err(error) => {
            return project_file_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
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
        if !canonical.starts_with(root) {
            continue;
        }
        entries.push(ProjectFileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: canonical
                .strip_prefix(root)
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
    Json(ProjectDirectoryResponse { path, entries }).into_response()
}

fn project_file_picker(
    pickers: &Mutex<HashMap<PathBuf, SharedFilePicker>>,
    project_dir: &Path,
) -> Result<SharedFilePicker, String> {
    let project_dir = std::fs::canonicalize(project_dir).map_err(|error| error.to_string())?;
    let mut pickers = pickers
        .lock()
        .map_err(|_| "file search index lock was poisoned".to_string())?;
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

fn search_project_files(
    shared_picker: &SharedFilePicker,
    needle: &str,
    grep: bool,
) -> Result<ProjectSearchResponse, String> {
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
            .map(|file| ProjectSearchResult {
                path: file.relative_path(picker).replace('\\', "/"),
                line: None,
                column: None,
                preview: None,
            })
            .collect();
        return Ok(ProjectSearchResponse {
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
            ProjectSearchResult {
                path: file.relative_path(picker).replace('\\', "/"),
                line: Some(matched.line_number as usize),
                column: Some(matched.line_content[..matched.col].chars().count() + 1),
                preview: Some(matched.line_content.trim().chars().take(240).collect()),
            }
        })
        .collect();
    Ok(ProjectSearchResponse { results, limited })
}

fn project_file_error(status: StatusCode, message: impl Into<String>) -> Response<Body> {
    (
        status,
        Json(serde_json::json!({
            "error": message.into(),
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn project_paths_reject_parent_traversal() {
        let result = resolve_project_target(Path::new("C:/work/demo"), "../secret.txt").await;
        assert!(matches!(
            result,
            Err(ProjectFileError {
                status: StatusCode::BAD_REQUEST,
                ..
            })
        ));
    }

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
        assert!(picker.wait_for_indexing_complete(Duration::from_secs(10)));

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
