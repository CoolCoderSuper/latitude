use std::{path::Path, sync::Arc, time::Instant};

use axum::extract::ws::{Message, WebSocket};
use serde::{Deserialize, Serialize};
use tokio::{process::Command, sync::broadcast, time::timeout};

use crate::terminal::{TerminalSession, TerminalSessionSummary, root_terminal_cwd, terminal_cwd};
use crate::workspace::{WorkspaceExecRequest, global_workspace_bridge};

use super::{
    constants::{
        MAX_TERMINAL_COMMAND_BYTES, MAX_TERMINAL_OUTPUT_BYTES, PUBLIC_API_PROJECTS_PATH,
        PUBLIC_API_ROOT_TERMINAL_SESSIONS_PATH, TERMINAL_COMMAND_TIMEOUT,
    },
    page::{content_type_media_type, is_json_media_type},
    paths::display_path,
};

#[derive(Debug, Serialize)]
pub(super) struct PublicTerminalInfoResponse {
    pub(super) cwd: String,
    pub(super) shell: &'static str,
    pub(super) timeout_seconds: u64,
    pub(super) max_output_bytes: usize,
    pub(super) sessions_href: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicTerminalSessionListResponse {
    pub(super) sessions: Vec<TerminalSessionSummary>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalCommandPayload {
    pub(super) command: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalWsQuery {
    pub(super) token: Option<String>,
    pub(super) session: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum TerminalClientMessage {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug, Serialize)]
pub(super) struct PublicTerminalCommandResponse {
    pub(super) command: String,
    pub(super) cwd: String,
    pub(super) shell: &'static str,
    pub(super) exit_code: Option<i32>,
    pub(super) success: bool,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) duration_ms: u128,
    pub(super) timed_out: bool,
}

pub(super) fn parse_terminal_command_payload(
    content_type: Option<&str>,
    body: &[u8],
) -> Result<String, String> {
    if content_type_media_type(content_type)
        .as_deref()
        .is_some_and(is_json_media_type)
    {
        let payload: TerminalCommandPayload = serde_json::from_slice(body)
            .map_err(|error| format!("terminal JSON payload is invalid: {error}"))?;
        return clean_terminal_command(payload.command);
    }

    let mut command = None;
    for (key, value) in url::form_urlencoded::parse(body) {
        if key == "command" {
            command = Some(value.into_owned());
        }
    }

    clean_terminal_command(command.unwrap_or_default())
}

fn clean_terminal_command(command: String) -> Result<String, String> {
    let command = command.trim().to_string();
    if command.is_empty() {
        return Err("terminal command is required".to_string());
    }
    if command.len() > MAX_TERMINAL_COMMAND_BYTES {
        return Err(format!(
            "terminal command must be at most {MAX_TERMINAL_COMMAND_BYTES} bytes"
        ));
    }

    Ok(command)
}

pub(super) fn terminal_info_response(
    project: &str,
    project_dir: &Path,
) -> PublicTerminalInfoResponse {
    scoped_terminal_info_response(
        project_dir,
        format!("{PUBLIC_API_PROJECTS_PATH}/{project}/terminal/sessions"),
    )
}

pub(super) async fn root_terminal_info_response() -> PublicTerminalInfoResponse {
    let root_dir = if let Some(bridge) = global_workspace_bridge() {
        bridge.profile_dir().await.unwrap_or_else(root_terminal_cwd)
    } else {
        root_terminal_cwd()
    };
    scoped_terminal_info_response(
        &root_dir,
        PUBLIC_API_ROOT_TERMINAL_SESSIONS_PATH.to_string(),
    )
}

fn scoped_terminal_info_response(
    terminal_dir: &Path,
    sessions_href: String,
) -> PublicTerminalInfoResponse {
    let cwd = terminal_cwd(terminal_dir);
    PublicTerminalInfoResponse {
        cwd: display_path(&cwd),
        shell: terminal_shell_name(),
        timeout_seconds: TERMINAL_COMMAND_TIMEOUT.as_secs(),
        max_output_bytes: MAX_TERMINAL_OUTPUT_BYTES,
        sessions_href,
    }
}

pub(super) async fn execute_terminal_command(
    project_dir: Option<&Path>,
    command_text: String,
) -> PublicTerminalCommandResponse {
    let cwd = match project_dir {
        Some(project_dir) => terminal_cwd(project_dir),
        None => {
            if let Some(bridge) = global_workspace_bridge() {
                bridge.profile_dir().await.unwrap_or_else(root_terminal_cwd)
            } else {
                root_terminal_cwd()
            }
        }
    };

    if let Some(bridge) = global_workspace_bridge() {
        let started = Instant::now();
        let (program, args) = terminal_shell_parts(&command_text);
        let output = bridge
            .execute(
                WorkspaceExecRequest::captured(
                    program,
                    args,
                    Some(cwd.clone()),
                    TERMINAL_COMMAND_TIMEOUT,
                    MAX_TERMINAL_OUTPUT_BYTES,
                )
                .with_environment("NO_COLOR", "1"),
            )
            .await;
        return match output {
            Ok(output) => {
                let mut stderr = terminal_output_text(&output.stderr);
                if output.truncated {
                    if !stderr.is_empty() && !stderr.ends_with('\n') {
                        stderr.push('\n');
                    }
                    stderr.push_str(&format!(
                        "[workspace output truncated after {MAX_TERMINAL_OUTPUT_BYTES} bytes]"
                    ));
                }
                PublicTerminalCommandResponse {
                    command: command_text,
                    cwd: display_path(&cwd),
                    shell: terminal_shell_name(),
                    exit_code: output.status_code,
                    success: output.status_code == Some(0) && !output.timed_out,
                    stdout: terminal_output_text(&output.stdout),
                    stderr,
                    duration_ms: output.duration_ms,
                    timed_out: output.timed_out,
                }
            }
            Err(error) => PublicTerminalCommandResponse {
                command: command_text,
                cwd: display_path(&cwd),
                shell: terminal_shell_name(),
                exit_code: None,
                success: false,
                stdout: String::new(),
                stderr: format!("Could not run terminal command in the user workspace: {error}"),
                duration_ms: started.elapsed().as_millis(),
                timed_out: false,
            },
        };
    }

    let started = Instant::now();
    let mut command = terminal_shell_command(&command_text);
    command
        .current_dir(&cwd)
        .env("NO_COLOR", "1")
        .kill_on_drop(true);

    let result = timeout(TERMINAL_COMMAND_TIMEOUT, command.output()).await;
    let duration_ms = started.elapsed().as_millis();

    match result {
        Ok(Ok(output)) => PublicTerminalCommandResponse {
            command: command_text,
            cwd: display_path(&cwd),
            shell: terminal_shell_name(),
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: terminal_output_text(&output.stdout),
            stderr: terminal_output_text(&output.stderr),
            duration_ms,
            timed_out: false,
        },
        Ok(Err(error)) => PublicTerminalCommandResponse {
            command: command_text,
            cwd: display_path(&cwd),
            shell: terminal_shell_name(),
            exit_code: None,
            success: false,
            stdout: String::new(),
            stderr: format!("Could not run terminal shell: {error}"),
            duration_ms,
            timed_out: false,
        },
        Err(_) => PublicTerminalCommandResponse {
            command: command_text,
            cwd: display_path(&cwd),
            shell: terminal_shell_name(),
            exit_code: None,
            success: false,
            stdout: String::new(),
            stderr: format!(
                "Command timed out after {} seconds",
                TERMINAL_COMMAND_TIMEOUT.as_secs()
            ),
            duration_ms,
            timed_out: true,
        },
    }
}

fn terminal_shell_name() -> &'static str {
    if cfg!(windows) { "powershell" } else { "sh" }
}

fn terminal_shell_command(command_text: &str) -> Command {
    let (program, args) = terminal_shell_parts(command_text);
    let mut command = Command::new(program);
    command.args(args);
    command
}

fn terminal_shell_parts(command_text: &str) -> (&'static str, Vec<String>) {
    if cfg!(windows) {
        (
            "powershell.exe",
            [
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                command_text,
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        )
    } else {
        (
            "sh",
            ["-lc", command_text]
                .into_iter()
                .map(str::to_string)
                .collect(),
        )
    }
}

fn terminal_output_text(bytes: &[u8]) -> String {
    let truncated = bytes.len() > MAX_TERMINAL_OUTPUT_BYTES;
    let visible = if truncated {
        &bytes[..MAX_TERMINAL_OUTPUT_BYTES]
    } else {
        bytes
    };
    let mut output = String::from_utf8_lossy(visible).to_string();
    if truncated {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!(
            "[output truncated after {MAX_TERMINAL_OUTPUT_BYTES} bytes]"
        ));
    }

    output
}

pub(crate) async fn terminal_websocket_session(
    mut socket: WebSocket,
    session: Arc<TerminalSession>,
) {
    session.attach_client();
    for output in session.history() {
        if socket
            .send(Message::Text(
                String::from_utf8_lossy(&output).to_string().into(),
            ))
            .await
            .is_err()
        {
            session.detach_client();
            return;
        }
    }

    let mut output_rx = session.subscribe();

    loop {
        tokio::select! {
            output = output_rx.recv() => {
                match output {
                    Ok(output) => {
                        if socket
                            .send(Message::Text(String::from_utf8_lossy(&output).to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            message = socket.recv() => {
                let Some(message) = message else {
                    break;
                };
                let Ok(message) = message else {
                    break;
                };

                match message {
                    Message::Text(text) => {
                        if let Ok(payload) = serde_json::from_str::<TerminalClientMessage>(&text) {
                            handle_terminal_client_message(payload, &session);
                        }
                    }
                    Message::Binary(bytes) => {
                        session.write_input(&String::from_utf8_lossy(&bytes));
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }

    session.detach_client();
}

fn handle_terminal_client_message(payload: TerminalClientMessage, session: &TerminalSession) {
    match payload {
        TerminalClientMessage::Input { data } => {
            session.write_input(&data);
        }
        TerminalClientMessage::Resize { cols, rows } => {
            session.resize(cols, rows);
        }
    }
}
