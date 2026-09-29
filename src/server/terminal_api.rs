use std::{
    collections::VecDeque,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::terminal::{TerminalOutput, TerminalSession, TerminalSessionSummary, terminal_cwd};
use crate::workspace::{WorkspaceExecRequest, execute_process, profile_dir};

use super::{
    constants::{
        MAX_TERMINAL_COMMAND_BYTES, MAX_TERMINAL_OUTPUT_BYTES, PUBLIC_API_PROJECTS_PATH,
        PUBLIC_API_ROOT_TERMINAL_SESSIONS_PATH, TERMINAL_COMMAND_TIMEOUT,
    },
    page::{content_type_media_type, is_json_media_type},
    paths::display_path,
};

const TERMINAL_HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const TERMINAL_OUTPUT_FRAME_KIND: u8 = 1;
const TERMINAL_OUTPUT_FRAME_HEADER_BYTES: usize = 17;
const TERMINAL_OUTPUT_FRAME_PAYLOAD_BYTES: usize = 64 * 1024;
const TERMINAL_MAX_UNACKNOWLEDGED_BYTES: usize = 256 * 1024;
const TERMINAL_MAX_PENDING_BYTES: usize = 2 * 1024 * 1024;

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
    Hello {
        last_sequence: Option<u64>,
        cols: u16,
        rows: u16,
    },
    Input {
        data: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Ack {
        sequence: u64,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalServerMessage {
    Ready { reset: bool, sequence: u64 },
}

#[derive(Debug)]
struct TerminalHello {
    last_sequence: Option<u64>,
    cols: u16,
    rows: u16,
}

#[derive(Debug)]
struct EncodedTerminalOutput {
    bytes: Bytes,
    end_sequence: u64,
    payload_bytes: usize,
}

#[derive(Debug)]
struct SentTerminalOutput {
    end_sequence: u64,
    payload_bytes: usize,
}

#[derive(Default, Debug)]
struct TerminalDeliveryWindow {
    sent: VecDeque<SentTerminalOutput>,
    unacknowledged_bytes: usize,
    last_acknowledged_sequence: u64,
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
    let root_dir = profile_dir().await;
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
        None => profile_dir().await,
    };

    let started = Instant::now();
    let (program, args) = terminal_shell_parts(&command_text);
    let output = execute_process(
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
    match output {
        Ok(output) => {
            let mut stderr = terminal_output_text(&output.stderr);
            if output.timed_out {
                stderr = format!(
                    "Command timed out after {} seconds",
                    TERMINAL_COMMAND_TIMEOUT.as_secs()
                );
            } else if output.truncated {
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
            stderr: format!("Could not run terminal command: {error}"),
            duration_ms: started.elapsed().as_millis(),
            timed_out: false,
        },
    }
}

fn terminal_shell_name() -> &'static str {
    if cfg!(windows) { "powershell" } else { "sh" }
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
    let client_id = session.attach_client();
    // Subscribe before the client handshake and retained replay so output cannot
    // fall into a gap while either is in progress.
    let mut output_rx = session.subscribe();
    let Some(hello) = receive_terminal_hello(&mut socket, &session, client_id).await else {
        session.detach_client(client_id);
        return;
    };
    session.resize(client_id, hello.cols, hello.rows);

    let replay = session.replay_after(hello.last_sequence);
    let mut last_queued_sequence = replay.base_sequence;
    let mut pending_bytes = replay
        .outputs
        .iter()
        .map(|output| output.bytes().len())
        .sum::<usize>();
    let mut pending = VecDeque::from(replay.outputs);
    if let Some(output) = pending.back() {
        last_queued_sequence = output.sequence();
    }

    let ready = TerminalServerMessage::Ready {
        reset: replay.reset,
        sequence: replay.base_sequence,
    };
    let Ok(ready) = serde_json::to_string(&ready) else {
        session.detach_client(client_id);
        return;
    };
    if socket.send(Message::Text(ready.into())).await.is_err() {
        session.detach_client(client_id);
        return;
    }

    let mut delivery = TerminalDeliveryWindow::default();
    'connection: loop {
        while delivery.can_send() && !pending.is_empty() {
            let Some(output) = take_terminal_output_frame(&mut pending, &mut pending_bytes) else {
                break;
            };
            if socket
                .send(Message::Binary(output.bytes.clone()))
                .await
                .is_err()
            {
                break 'connection;
            }
            delivery.record_sent(output);
        }

        tokio::select! {
            output = output_rx.recv() => {
                match output {
                    Ok(output) => {
                        if output.sequence() <= last_queued_sequence {
                            continue;
                        }
                        if !queue_terminal_output(
                            output,
                            &mut pending,
                            &mut pending_bytes,
                            &mut last_queued_sequence,
                        ) {
                            close_terminal_for_resynchronization(&mut socket).await;
                            break 'connection;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let recovery = session.replay_after(Some(last_queued_sequence));
                        if recovery.reset {
                            close_terminal_for_resynchronization(&mut socket).await;
                            break 'connection;
                        }
                        for output in recovery.outputs {
                            if !queue_terminal_output(
                                output,
                                &mut pending,
                                &mut pending_bytes,
                                &mut last_queued_sequence,
                            ) {
                                close_terminal_for_resynchronization(&mut socket).await;
                                break 'connection;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break 'connection,
                }
            }
            message = socket.recv() => {
                let Some(message) = message else {
                    break 'connection;
                };
                let Ok(message) = message else {
                    break 'connection;
                };

                match message {
                    Message::Text(text) => {
                        if let Ok(payload) = serde_json::from_str::<TerminalClientMessage>(&text) {
                            match payload {
                                TerminalClientMessage::Ack { sequence } => {
                                    if delivery.acknowledge(sequence) {
                                        session.acknowledge_output(sequence);
                                    }
                                }
                                TerminalClientMessage::Hello { .. } => {}
                                payload => handle_terminal_client_message(payload, &session, client_id),
                            }
                        }
                    }
                    Message::Binary(bytes) => {
                        session.write_input_bytes(&bytes);
                    }
                    Message::Close(_) => break 'connection,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }

    session.detach_client(client_id);
}

async fn receive_terminal_hello(
    socket: &mut WebSocket,
    session: &TerminalSession,
    client_id: u64,
) -> Option<TerminalHello> {
    loop {
        let message = timeout(TERMINAL_HELLO_TIMEOUT, socket.recv())
            .await
            .ok()??
            .ok()?;
        match message {
            Message::Text(text) => {
                let payload = serde_json::from_str::<TerminalClientMessage>(&text).ok()?;
                match payload {
                    TerminalClientMessage::Hello {
                        last_sequence,
                        cols,
                        rows,
                    } => {
                        return Some(TerminalHello {
                            last_sequence,
                            cols,
                            rows,
                        });
                    }
                    TerminalClientMessage::Ack { .. } => {}
                    payload => handle_terminal_client_message(payload, session, client_id),
                }
            }
            Message::Binary(bytes) => session.write_input_bytes(&bytes),
            Message::Close(_) => return None,
            Message::Ping(_) | Message::Pong(_) => {}
        }
    }
}

fn queue_terminal_output(
    output: TerminalOutput,
    pending: &mut VecDeque<TerminalOutput>,
    pending_bytes: &mut usize,
    last_queued_sequence: &mut u64,
) -> bool {
    if output.sequence() <= *last_queued_sequence {
        return true;
    }
    let next_pending_bytes = pending_bytes.saturating_add(output.bytes().len());
    if next_pending_bytes > TERMINAL_MAX_PENDING_BYTES {
        return false;
    }
    *pending_bytes = next_pending_bytes;
    *last_queued_sequence = output.sequence();
    pending.push_back(output);
    true
}

fn take_terminal_output_frame(
    pending: &mut VecDeque<TerminalOutput>,
    pending_bytes: &mut usize,
) -> Option<EncodedTerminalOutput> {
    let first_sequence = pending.front()?.sequence();
    let mut end_sequence = first_sequence;
    let mut payload = Vec::with_capacity(TERMINAL_OUTPUT_FRAME_PAYLOAD_BYTES);

    while let Some(output) = pending.front() {
        let output_bytes = output.bytes();
        if !payload.is_empty()
            && payload.len().saturating_add(output_bytes.len())
                > TERMINAL_OUTPUT_FRAME_PAYLOAD_BYTES
        {
            break;
        }
        let output = pending.pop_front().expect("pending output disappeared");
        end_sequence = output.sequence();
        payload.extend_from_slice(output.bytes());
        *pending_bytes = pending_bytes.saturating_sub(output.bytes().len());
    }

    let payload_bytes = payload.len();
    let mut frame = Vec::with_capacity(TERMINAL_OUTPUT_FRAME_HEADER_BYTES + payload_bytes);
    frame.push(TERMINAL_OUTPUT_FRAME_KIND);
    frame.extend_from_slice(&first_sequence.to_be_bytes());
    frame.extend_from_slice(&end_sequence.to_be_bytes());
    frame.extend_from_slice(&payload);
    Some(EncodedTerminalOutput {
        bytes: Bytes::from(frame),
        end_sequence,
        payload_bytes,
    })
}

async fn close_terminal_for_resynchronization(socket: &mut WebSocket) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: close_code::AGAIN,
            reason: "terminal output fell behind; reconnect to resynchronize".into(),
        })))
        .await;
}

impl TerminalDeliveryWindow {
    fn can_send(&self) -> bool {
        self.unacknowledged_bytes < TERMINAL_MAX_UNACKNOWLEDGED_BYTES
    }

    fn record_sent(&mut self, output: EncodedTerminalOutput) {
        self.unacknowledged_bytes = self
            .unacknowledged_bytes
            .saturating_add(output.payload_bytes);
        self.sent.push_back(SentTerminalOutput {
            end_sequence: output.end_sequence,
            payload_bytes: output.payload_bytes,
        });
    }

    fn acknowledge(&mut self, sequence: u64) -> bool {
        if sequence <= self.last_acknowledged_sequence {
            return false;
        }
        if self
            .sent
            .back()
            .is_none_or(|output| sequence > output.end_sequence)
        {
            return false;
        }
        while self
            .sent
            .front()
            .is_some_and(|output| output.end_sequence <= sequence)
        {
            let output = self.sent.pop_front().expect("sent output disappeared");
            self.unacknowledged_bytes = self
                .unacknowledged_bytes
                .saturating_sub(output.payload_bytes);
        }
        self.last_acknowledged_sequence = sequence;
        true
    }
}

fn handle_terminal_client_message(
    payload: TerminalClientMessage,
    session: &TerminalSession,
    client_id: u64,
) {
    match payload {
        TerminalClientMessage::Hello { .. } | TerminalClientMessage::Ack { .. } => {}
        TerminalClientMessage::Input { data } => {
            session.write_input(&data);
        }
        TerminalClientMessage::Resize { cols, rows } => {
            session.resize(client_id, cols, rows);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(sequence: u64, bytes: &'static [u8]) -> TerminalOutput {
        TerminalOutput::for_test(sequence, Bytes::from_static(bytes))
    }

    #[test]
    fn terminal_output_frames_are_sequenced_and_coalesced() {
        let mut pending = VecDeque::from([output(7, b"hello "), output(8, b"world")]);
        let mut pending_bytes = 11;

        let encoded = take_terminal_output_frame(&mut pending, &mut pending_bytes).unwrap();

        assert_eq!(encoded.bytes[0], TERMINAL_OUTPUT_FRAME_KIND);
        assert_eq!(
            u64::from_be_bytes(encoded.bytes[1..9].try_into().unwrap()),
            7
        );
        assert_eq!(
            u64::from_be_bytes(encoded.bytes[9..17].try_into().unwrap()),
            8
        );
        assert_eq!(&encoded.bytes[17..], b"hello world");
        assert_eq!(encoded.payload_bytes, 11);
        assert_eq!(pending_bytes, 0);
        assert!(pending.is_empty());
    }

    #[test]
    fn terminal_delivery_window_reopens_after_parser_ack() {
        let mut delivery = TerminalDeliveryWindow::default();
        delivery.record_sent(EncodedTerminalOutput {
            bytes: Bytes::new(),
            end_sequence: 4,
            payload_bytes: TERMINAL_MAX_UNACKNOWLEDGED_BYTES,
        });

        assert!(!delivery.can_send());
        delivery.acknowledge(4);
        assert!(delivery.can_send());
        assert_eq!(delivery.unacknowledged_bytes, 0);
    }
}
