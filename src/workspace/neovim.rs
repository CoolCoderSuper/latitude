use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures_util::SinkExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{Mutex, mpsc},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest},
};

use super::{WorkspaceHostState, WorkspaceServices};

#[derive(Default)]
pub(super) struct NeovimSessions {
    sessions: Mutex<HashMap<PathBuf, mpsc::Sender<WebSocket>>>,
}

impl NeovimSessions {
    async fn attach(&self, mut socket: WebSocket, cwd: PathBuf) {
        loop {
            let sender = {
                let mut sessions = self.sessions.lock().await;
                sessions.retain(|_, sender| !sender.is_closed());
                sessions
                    .entry(cwd.clone())
                    .or_insert_with(|| {
                        let (sender, mut connections) = mpsc::channel(4);
                        let cwd = cwd.clone();
                        tokio::spawn(async move {
                            let mut socket = connections.recv().await;
                            if socket.is_none() {
                                return;
                            }
                            let result = run_local(&mut socket, cwd, &mut connections).await;
                            if let Some(mut socket) = socket {
                                finish(&mut socket, result).await;
                            }
                        });
                        sender
                    })
                    .clone()
            };
            match sender.send(socket).await {
                Ok(()) => return,
                // The process may have exited between lookup and attachment.
                Err(error) => socket = error.0,
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Input {
    OpenFile {
        path: String,
    },
    Input {
        keys: String,
    },
    Paste {
        text: String,
    },
    Resize {
        width: u32,
        height: u32,
    },
    Mouse {
        button: String,
        action: String,
        modifiers: String,
        row: u32,
        col: u32,
    },
}

async fn open_file_rpc(cwd: &Path, path: &str) -> Result<Value> {
    let relative = Path::new(path);
    if path.is_empty()
        || path.len() > 16384
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        bail!("Choose a file inside the project directory");
    }
    let root = tokio::fs::canonicalize(cwd)
        .await
        .context("Project directory is unavailable")?;
    let target = tokio::fs::canonicalize(root.join(relative))
        .await
        .context("File is unavailable")?;
    if !target.starts_with(&root) || !tokio::fs::metadata(&target).await?.is_file() {
        bail!("Choose a file inside the project directory");
    }
    let path = crate::util::strip_windows_extended_path(&target.to_string_lossy()).into_owned();
    // The path is a Lua argument, never executable Ex/Lua text. :hide keeps the
    // previous buffer alive even when it has unsaved changes and 'hidden' is off.
    Ok(json!([
        0,
        3,
        "nvim_exec_lua",
        [
            "local path = ...; local buf = vim.fn.bufadd(path); vim.fn.bufload(buf); vim.bo[buf].buflisted = true; vim.cmd('stopinsert'); vim.cmd('hide buffer ' .. buf); return buf",
            [path]
        ]
    ]))
}

impl Input {
    fn rpc(self) -> Result<(&'static str, Value)> {
        Ok(match self {
            Self::Input { keys } if keys.len() <= 65536 => ("nvim_input", json!([keys])),
            Self::Paste { text } if text.len() <= 512 * 1024 => {
                ("nvim_paste", json!([text, true, -1]))
            }
            Self::Resize { width, height }
                if (2..=500).contains(&width) && (2..=300).contains(&height) =>
            {
                ("nvim_ui_try_resize", json!([width, height]))
            }
            Self::Mouse {
                button,
                action,
                modifiers,
                row,
                col,
            } if ["left", "right", "middle", "wheel"].contains(&button.as_str())
                && ["press", "drag", "release", "up", "down", "left", "right"]
                    .contains(&action.as_str())
                && modifiers.chars().all(|c| "SCAM-".contains(c))
                && modifiers.len() <= 8
                && row < 300
                && col < 500 =>
            {
                (
                    "nvim_input_mouse",
                    json!([button, action, modifiers, 0, row, col]),
                )
            }
            _ => bail!("Invalid Neovim input"),
        })
    }
}

impl WorkspaceServices {
    pub(crate) async fn run_neovim(&self, mut socket: WebSocket, cwd: PathBuf) {
        if self.bridge.is_none() {
            self.neovim.attach(socket, cwd).await;
            return;
        }
        let result = if let Some(bridge) = &self.bridge {
            async {
                let endpoint = bridge.endpoint().await?;
                let mut request =
                    format!("ws://{}/neovim", endpoint.address).into_client_request()?;
                request.headers_mut().insert(
                    "authorization",
                    format!("Bearer {}", endpoint.token).parse()?,
                );
                let (mut worker, _) = connect_async(request)
                    .await
                    .context("Neovim workspace host is unavailable")?;
                worker
                    .send(tungstenite::Message::Text(
                        serde_json::to_string(&cwd)?.into(),
                    ))
                    .await?;
                crate::websocket_bridge::forward_websocket(&mut socket, &mut worker).await?;
                Ok(())
            }
            .await
        } else {
            unreachable!("local connections attach through the session manager")
        };
        finish(&mut socket, result).await;
    }
}

pub(super) async fn workspace_neovim(
    State(state): State<WorkspaceHostState>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.max_message_size(1024 * 1024)
        .on_upgrade(move |mut socket| async move {
            let result = async {
                let Some(Ok(Message::Text(text))) =
                    tokio::time::timeout(Duration::from_secs(10), socket.recv()).await?
                else {
                    bail!("Missing Neovim project directory");
                };
                Ok(serde_json::from_str::<PathBuf>(&text)?)
            }
            .await;
            match result {
                Ok(cwd) => state.neovim.attach(socket, cwd).await,
                Err(error) => finish(&mut socket, Err(error)).await,
            }
        })
}

async fn finish(socket: &mut WebSocket, result: Result<()>) {
    let message = match result {
        Ok(()) => json!({"type": "exit"}),
        Err(error) => json!({"type": "error", "message": format!("{error:#}")}),
    };
    let _ = socket.send(Message::Text(message.to_string().into())).await;
    let _ = socket.send(Message::Close(None)).await;
}

// Decode complete MessagePack objects, retaining an incomplete tail between reads.
fn decode(buffer: &mut Vec<u8>) -> Result<Option<Value>> {
    let mut cursor = Cursor::new(buffer.as_slice());
    match rmpv::decode::read_value(&mut cursor) {
        Ok(value) => {
            let consumed = cursor.position() as usize;
            buffer.drain(..consumed);
            // Neovim includes MessagePack extension values for window and buffer
            // handles in UI events. Preserve them as JSON tuples; the grid renderer
            // does not need to interpret these opaque handles.
            Ok(Some(serde_json::to_value(value)?))
        }
        Err(
            rmpv::decode::Error::InvalidMarkerRead(error)
            | rmpv::decode::Error::InvalidDataRead(error),
        ) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn send_ui(socket: &mut Option<WebSocket>, message: Value) {
    if let Some(client) = socket {
        if !matches!(
            tokio::time::timeout(
                Duration::from_secs(5),
                client.send(Message::Text(message.to_string().into()))
            )
            .await,
            Ok(Ok(()))
        ) {
            *socket = None;
        }
    }
}

async fn run_local(
    socket: &mut Option<WebSocket>,
    cwd: PathBuf,
    connections: &mut mpsc::Receiver<WebSocket>,
) -> Result<()> {
    let mut command = Command::new("nvim");
    command
        // Open the directory after init.lua has finished. Passing it as the
        // initial file lets directory plugins run during config-created tabs.
        .args(["--embed", "-c", "edit ."])
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let mut child = command.spawn().context(
        "Could not start Neovim. Install nvim on the workspace host and make it available on PATH",
    )?;
    let mut stdin = child.stdin.take().context("Neovim stdin is unavailable")?;
    let mut stdout = child
        .stdout
        .take()
        .context("Neovim stdout is unavailable")?;
    let mut stderr = child
        .stderr
        .take()
        .context("Neovim stderr is unavailable")?;
    let result = async {
        // The attach request is acknowledged before the UI is marked ready.
        // Keystrokes use notifications so commands can still receive input.
        stdin.write_all(&rmp_serde::to_vec(&json!([0, 1, "nvim_ui_attach", [100, 30, {
            "rgb": true, "ext_linegrid": true
        }]]))?).await?;
        let mut buffer = Vec::new();
        let mut chunk = [0; 65536];
        let mut error_chunk = [0; 4096];
        let mut diagnostic = String::new();
        let mut stderr_open = true;
        let mut operation_pending = false;
        let mut queued_input = VecDeque::new();
        let mut queued_bytes = 0;
        loop {
            tokio::select! {
                // Fast input can overtake a paste or buffer switch.
                // Delay later keys until it completes. The actor keeps running
                // without a browser so buffers and background jobs stay alive.
                _ = std::future::ready(()), if !operation_pending && !queued_input.is_empty() => {
                    let (input, bytes): (Input, usize) = queued_input.pop_front().unwrap();
                    queued_bytes -= bytes;
                    if let Input::OpenFile { path } = input {
                        match open_file_rpc(&cwd, &path).await {
                            Ok(request) => {
                                operation_pending = true;
                                stdin.write_all(&rmp_serde::to_vec(&request)?).await?;
                            }
                            Err(error) => send_ui(socket, json!({"type":"open_error", "message":format!("{error:#}")})).await,
                        }
                        continue;
                    }
                    let (method, args) = input.rpc()?;
                    let request = if method == "nvim_paste" {
                        operation_pending = true;
                        json!([0, 2, method, args])
                    } else {
                        json!([2, method, args])
                    };
                    stdin.write_all(&rmp_serde::to_vec(&request)?).await?;
                }
                connection = connections.recv() => {
                    let Some(connection) = connection else { break; };
                    send_ui(socket, json!({"type":"detached", "message":"This session is open in another tab."})).await;
                    if let Some(mut previous) = socket.take() {
                        let _ = previous.send(Message::Close(None)).await;
                    }
                    *socket = Some(connection);
                    // A fresh UI attachment makes Neovim resend the entire grid,
                    // highlights and mode, without reopening buffers or config.
                    stdin.write_all(&rmp_serde::to_vec(&json!([2, "nvim_ui_detach", []]))?).await?;
                    stdin.write_all(&rmp_serde::to_vec(&json!([0, 1, "nvim_ui_attach", [100, 30, {
                        "rgb": true, "ext_linegrid": true
                    }]]))?).await?;
                }
                incoming = async { socket.as_mut().unwrap().recv().await }, if socket.is_some() => {
                    match incoming {
                        Some(Ok(Message::Text(text))) => {
                            queued_bytes += text.len();
                            if queued_bytes > 2 * 1024 * 1024 || queued_input.len() >= 4096 {
                                bail!("Neovim input queue is full");
                            }
                            queued_input.push_back((serde_json::from_str::<Input>(&text)?, text.len()));
                        }
                        Some(Ok(Message::Ping(bytes))) => {
                            if socket.as_mut().unwrap().send(Message::Pong(bytes)).await.is_err() { *socket = None; }
                        },
                        Some(Ok(Message::Pong(_))) => {},
                        _ => *socket = None,
                    }
                }
                read = stdout.read(&mut chunk) => {
                    let count = read?;
                    if count == 0 {
                        let status = child.wait().await?;
                        if !status.success() { bail!("Neovim exited with {status}. {diagnostic}"); }
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..count]);
                    if buffer.len() > 16 * 1024 * 1024 { bail!("Neovim UI message exceeds 16 MiB"); }
                    while let Some(value) = decode(&mut buffer)? {
                        if value[0] == 1 && value[1] == 1 {
                            if !value[2].is_null() { bail!("Neovim could not attach its UI: {}", value[2]); }
                            send_ui(socket, json!({"type":"ready"})).await;
                        } else if value[0] == 1 && value[1] == 2 {
                            operation_pending = false;
                            if !value[2].is_null() { bail!("Neovim could not paste: {}", value[2]); }
                        } else if value[0] == 1 && value[1] == 3 {
                            operation_pending = false;
                            let message = if value[2].is_null() { json!({"type":"file_opened"}) }
                                else { json!({"type":"open_error", "message":format!("Could not open file: {}", value[2])}) };
                            send_ui(socket, message).await;
                        } else if value[0] == 2 && value[1] == "redraw" {
                            send_ui(socket, json!({"type":"redraw", "events":value[2]})).await;
                        } else if value[0] == 0 {
                            // Do not leave a plugin's unsupported GUI request hanging.
                            stdin.write_all(&rmp_serde::to_vec(&json!([1, value[1], [0, "Unsupported GUI request"], null]))?).await?;
                        }
                    }
                }
                read = stderr.read(&mut error_chunk), if stderr_open => {
                    let count = read?;
                    stderr_open = count != 0;
                    if diagnostic.len() < 8192 { diagnostic.push_str(&String::from_utf8_lossy(&error_chunk[..count])); }
                }
            }
        }
        Ok(())
    }.await;
    // Only process exit, an unrecoverable RPC error or host shutdown ends a session.
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_file_keeps_paths_in_the_project_and_out_of_ex_commands() {
        let root = std::env::temp_dir().join(format!(
            "latitude-nvim-open-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        tokio::fs::create_dir_all(root.join("nested"))
            .await
            .unwrap();
        let name = "nested/notes [v2] % λ.txt";
        tokio::fs::write(root.join(name), "test").await.unwrap();
        let request = open_file_rpc(&root, name).await.unwrap();
        assert_eq!(request[2], "nvim_exec_lua");
        assert!(!request[3][0].as_str().unwrap().contains("notes"));
        assert!(
            request[3][1][0]
                .as_str()
                .unwrap()
                .ends_with("notes [v2] % λ.txt")
        );
        for path in [
            "../outside.txt",
            "nested/../outside.txt",
            "nested",
            "missing.txt",
            "",
        ] {
            assert!(open_file_rpc(&root, path).await.is_err(), "{path}");
        }
        assert!(
            open_file_rpc(&root, root.join(name).to_str().unwrap())
                .await
                .is_err()
        );
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[test]
    fn fragmented_and_coalesced_rpc_messages() {
        let event = json!([
            2,
            "redraw",
            [["grid_line", [1, 0, 0, [["λ", 3, 2]], false]]]
        ]);
        let bytes = rmp_serde::to_vec(&event).unwrap();
        for split in 0..bytes.len() {
            let mut buffer = bytes[..split].to_vec();
            assert!(decode(&mut buffer).unwrap().is_none());
            buffer.extend_from_slice(&bytes[split..]);
            buffer.extend_from_slice(&bytes);
            assert_eq!(decode(&mut buffer).unwrap(), Some(event.clone()));
            assert_eq!(decode(&mut buffer).unwrap(), Some(event.clone()));
            assert!(buffer.is_empty());
        }
    }

    #[test]
    fn decodes_neovim_extension_handles() {
        let event = rmpv::Value::Array(vec![
            2.into(),
            "redraw".into(),
            rmpv::Value::Array(vec![rmpv::Value::Array(vec![
                "win_viewport".into(),
                rmpv::Value::Array(vec![1.into(), rmpv::Value::Ext(1, vec![0xcd, 0x03, 0xe8])]),
            ])]),
        ]);
        let mut bytes = Vec::new();
        rmpv::encode::write_value(&mut bytes, &event).unwrap();
        let decoded = decode(&mut bytes).unwrap().unwrap();
        assert_eq!(decoded[1], "redraw");
        assert_eq!(decoded[2][0][1][1], json!([1, [205, 3, 232]]));
        assert!(bytes.is_empty());
    }

    #[test]
    fn rejects_unbounded_dimensions_and_unknown_operations() {
        assert!(
            Input::Resize {
                width: 0,
                height: 30
            }
            .rpc()
            .is_err()
        );
        assert!(
            Input::Resize {
                width: 500,
                height: 300
            }
            .rpc()
            .is_ok()
        );
        assert!(
            Input::Resize {
                width: 501,
                height: 30
            }
            .rpc()
            .is_err()
        );
        assert!(
            serde_json::from_value::<Input>(json!({"type":"command", "command":"quit"})).is_err()
        );
    }
}
