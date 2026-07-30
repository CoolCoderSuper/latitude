use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use super::{
    WorkspaceBridge, WorkspaceEndpoint, WorkspaceExecRequest, WorkspaceExecResponse,
    WorkspaceHealth, WorkspaceProcessOutput, process::WORKSPACE_EXEC_PATH,
};

impl WorkspaceBridge {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn set_endpoint(
        &self,
        address: SocketAddr,
        token: impl Into<String>,
        health: WorkspaceHealth,
    ) {
        *self.endpoint.write().await = Some(WorkspaceEndpoint {
            address,
            token: token.into(),
            profile_dir: health.profile_dir,
        });
    }

    pub(crate) async fn clear_endpoint(&self) {
        self.endpoint.write().await.take();
    }

    pub(crate) async fn profile_dir(&self) -> Option<PathBuf> {
        self.endpoint
            .read()
            .await
            .as_ref()
            .map(|endpoint| endpoint.profile_dir.clone())
    }

    pub(super) async fn endpoint(&self) -> Result<WorkspaceEndpoint> {
        self.endpoint
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("no signed-in Windows workspace session is available"))
    }

    pub(crate) async fn execute(
        &self,
        request: WorkspaceExecRequest,
    ) -> Result<WorkspaceProcessOutput> {
        let endpoint = self.endpoint().await?;
        let url = format!("http://{}{}", endpoint.address, WORKSPACE_EXEC_PATH);
        let response = reqwest::Client::new()
            .post(url)
            .bearer_auth(&endpoint.token)
            .json(&request)
            .send()
            .await
            .context("workspace process host is unavailable")?;
        let response = workspace_success_response(response).await?;
        let response: WorkspaceExecResponse = response
            .json()
            .await
            .context("workspace process response was invalid")?;
        Ok(WorkspaceProcessOutput {
            status_code: response.status_code,
            stdout: BASE64
                .decode(response.stdout)
                .context("workspace stdout was not valid base64")?,
            stderr: BASE64
                .decode(response.stderr)
                .context("workspace stderr was not valid base64")?,
            duration_ms: response.duration_ms,
            timed_out: response.timed_out,
            truncated: response.truncated,
        })
    }
}

pub(super) async fn workspace_success_response(
    response: reqwest::Response,
) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let detail = response.text().await.unwrap_or_default();
    Err(anyhow!(
        "workspace host returned {status}: {}",
        detail.trim()
    ))
}
