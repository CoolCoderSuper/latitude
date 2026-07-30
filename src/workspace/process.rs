use std::{
    process::Stdio,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, Response, StatusCode},
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use tokio::{process::Command, time::timeout};

use crate::terminal::{root_terminal_cwd, terminal_cwd};

use super::{
    WorkspaceExecRequest, WorkspaceExecResponse, WorkspaceHostState, WorkspaceProcessOutput,
    host::{workspace_error, workspace_is_authenticated},
};

pub(super) const WORKSPACE_EXEC_PATH: &str = "/exec";
const MAX_WORKSPACE_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

impl WorkspaceExecRequest {
    pub(crate) fn captured(
        program: impl Into<String>,
        args: Vec<String>,
        cwd: Option<std::path::PathBuf>,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            cwd,
            environment: Vec::new(),
            timeout_ms: timeout.as_millis().clamp(1, u128::from(u64::MAX)) as u64,
            max_output_bytes: max_output_bytes.clamp(1, MAX_WORKSPACE_OUTPUT_BYTES),
            detached: false,
        }
    }

    pub(crate) fn detached(
        program: impl Into<String>,
        args: Vec<String>,
        cwd: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            cwd,
            environment: Vec::new(),
            timeout_ms: 1,
            max_output_bytes: 1,
            detached: true,
        }
    }

    pub(crate) fn with_environment(mut self, name: &str, value: &str) -> Self {
        self.environment.push((name.to_string(), value.to_string()));
        self
    }
}

pub(super) async fn workspace_exec(
    State(state): State<WorkspaceHostState>,
    headers: HeaderMap,
    Json(request): Json<WorkspaceExecRequest>,
) -> Response<Body> {
    if !workspace_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute_workspace_process(request).await {
        Ok(output) => Json(WorkspaceExecResponse {
            status_code: output.status_code,
            stdout: BASE64.encode(output.stdout),
            stderr: BASE64.encode(output.stderr),
            duration_ms: output.duration_ms,
            timed_out: output.timed_out,
            truncated: output.truncated,
        })
        .into_response(),
        Err(error) => workspace_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn execute_workspace_process(
    request: WorkspaceExecRequest,
) -> Result<WorkspaceProcessOutput> {
    if request.program.trim().is_empty() {
        return Err(anyhow!("workspace process program is required"));
    }
    let cwd = request
        .cwd
        .as_deref()
        .map(terminal_cwd)
        .unwrap_or_else(root_terminal_cwd);
    if !cwd.is_absolute() {
        return Err(anyhow!("workspace process directory must be absolute"));
    }

    let started = Instant::now();
    let mut command = Command::new(&request.program);
    command
        .args(&request.args)
        .current_dir(&cwd)
        .envs(request.environment);

    if request.detached {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(false);
        command
            .spawn()
            .with_context(|| format!("workspace process '{}' could not start", request.program))?;
        return Ok(WorkspaceProcessOutput {
            status_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            duration_ms: started.elapsed().as_millis(),
            timed_out: false,
            truncated: false,
        });
    }

    command.kill_on_drop(true);
    let command_timeout = Duration::from_millis(request.timeout_ms.clamp(1, 10 * 60 * 1_000));
    let output = match timeout(command_timeout, command.output()).await {
        Ok(output) => output
            .with_context(|| format!("workspace process '{}' could not start", request.program))?,
        Err(_) => {
            return Ok(WorkspaceProcessOutput {
                status_code: None,
                stdout: Vec::new(),
                stderr: Vec::new(),
                duration_ms: started.elapsed().as_millis(),
                timed_out: true,
                truncated: false,
            });
        }
    };
    let max_output = request
        .max_output_bytes
        .clamp(1, MAX_WORKSPACE_OUTPUT_BYTES);
    let (stdout, stdout_truncated) = truncate_bytes(output.stdout, max_output);
    let (stderr, stderr_truncated) = truncate_bytes(output.stderr, max_output);
    Ok(WorkspaceProcessOutput {
        status_code: output.status.code(),
        stdout,
        stderr,
        duration_ms: started.elapsed().as_millis(),
        timed_out: false,
        truncated: stdout_truncated || stderr_truncated,
    })
}

fn truncate_bytes(mut bytes: Vec<u8>, limit: usize) -> (Vec<u8>, bool) {
    let truncated = bytes.len() > limit;
    if truncated {
        bytes.truncate(limit);
    }
    (bytes, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_workspace_output_at_the_requested_boundary() {
        let (bytes, truncated) = truncate_bytes(vec![1, 2, 3, 4], 3);
        assert_eq!(bytes, [1, 2, 3]);
        assert!(truncated);

        let (bytes, truncated) = truncate_bytes(vec![1, 2, 3], 3);
        assert_eq!(bytes, [1, 2, 3]);
        assert!(!truncated);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn workspace_process_inherits_environment_and_uses_requested_directory() {
        let cwd = std::env::current_dir().unwrap();
        let request = WorkspaceExecRequest::captured(
            "powershell.exe",
            vec![
                "-NoLogo".to_string(),
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                "Write-Output \"$env:LATITUDE_WORKSPACE_TEST|$((Get-Location).Path)\"".to_string(),
            ],
            Some(cwd.clone()),
            Duration::from_secs(10),
            4096,
        )
        .with_environment("LATITUDE_WORKSPACE_TEST", "user-context");

        let output = execute_workspace_process(request).await.unwrap();
        assert_eq!(output.status_code, Some(0));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("user-context|"));
        assert!(
            stdout.to_lowercase().contains(
                &cwd.to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .to_lowercase()
            )
        );
    }
}
