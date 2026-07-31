use anyhow::{Context, Result, anyhow};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, Response, StatusCode, header},
    response::IntoResponse,
};
use serde::Serialize;

use crate::project_files::{ProjectFileError, ProjectFileRequest, ProjectFileWriteRequest};

use super::{
    WorkspaceBridge, WorkspaceHostState, WorkspaceServices,
    host::{workspace_error, workspace_is_authenticated},
};

pub(super) const WORKSPACE_FILES_PATH: &str = "/files";
pub(super) const WORKSPACE_FILE_WRITE_PATH: &str = "/files/write";
const MAX_INTERNAL_ERROR_BYTES: usize = 64 * 1024;

impl WorkspaceServices {
    pub(crate) async fn file_get(
        &self,
        request: ProjectFileRequest,
    ) -> Result<Response<Body>, ProjectFileError> {
        match &self.bridge {
            Some(bridge) => bridge.proxy_file_get(request).await.map_err(unavailable),
            None => Ok(self.files.get(request).await),
        }
    }

    pub(crate) async fn write_file(
        &self,
        request: ProjectFileWriteRequest,
    ) -> Result<(), ProjectFileError> {
        match &self.bridge {
            Some(bridge) => bridge.write_file(request).await.map_err(unavailable),
            None => self.files.write(request).await,
        }
    }
}

fn unavailable(error: anyhow::Error) -> ProjectFileError {
    ProjectFileError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        message: error.to_string(),
    }
}

impl WorkspaceBridge {
    pub(crate) async fn proxy_file_get(
        &self,
        request: ProjectFileRequest,
    ) -> Result<Response<Body>> {
        self.proxy_file_request(WORKSPACE_FILES_PATH, &request)
            .await
    }

    pub(crate) async fn write_file(&self, request: ProjectFileWriteRequest) -> Result<()> {
        let response = self
            .proxy_file_request(WORKSPACE_FILE_WRITE_PATH, &request)
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), MAX_INTERNAL_ERROR_BYTES)
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
        let response = self
            .client
            .post(url)
            .bearer_auth(&endpoint.token)
            .json(request)
            .send()
            .await
            .context("workspace file host is unavailable")?;
        let status = StatusCode::from_u16(response.status().as_u16())
            .context("workspace file status was invalid")?;
        let mut builder = Response::builder().status(status);
        for (name, value) in response.headers() {
            if matches!(
                name,
                &header::CONTENT_TYPE
                    | &header::CONTENT_LENGTH
                    | &header::CONTENT_RANGE
                    | &header::ACCEPT_RANGES
            ) {
                builder = builder.header(name, value);
            }
        }
        builder
            .body(Body::from_stream(response.bytes_stream()))
            .context("workspace file response could not be built")
    }
}

pub(super) async fn workspace_files(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    Json(request): Json<ProjectFileRequest>,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    state.files.get(request).await
}

pub(super) async fn workspace_file_write(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    Json(request): Json<ProjectFileWriteRequest>,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.files.write(request).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(error) => workspace_error(error.status, error.message),
    }
}
