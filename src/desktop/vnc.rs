use std::{
    net::IpAddr,
    time::{Duration, Instant},
};

use axum::extract::ws::{Message, WebSocket};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use tracing::{debug, warn};

use super::DesktopTarget;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const BRIDGE_BUFFER_SIZE: usize = 64 * 1024;

pub(super) async fn desktop_websocket_session(mut socket: WebSocket, target: DesktopTarget) {
    let address = vnc_address(&target.host, target.port);
    let connected_at = Instant::now();
    let stream = match timeout(CONNECT_TIMEOUT, TcpStream::connect(&address)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            warn!(%address, %error, "desktop VNC connection failed");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
        Err(_) => {
            warn!(%address, "desktop VNC connection timed out");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    if let Err(error) = stream.set_nodelay(true) {
        warn!(%address, %error, "desktop VNC bridge could not disable TCP buffering");
    }

    debug!(%address, "desktop VNC bridge connected");
    let (mut tcp_reader, mut tcp_writer) = stream.into_split();
    let mut buffer = vec![0_u8; BRIDGE_BUFFER_SIZE];

    loop {
        tokio::select! {
            read = tcp_reader.read(&mut buffer) => {
                match read {
                    Ok(0) => break,
                    Ok(count) => {
                        if socket
                            .send(Message::Binary(buffer[..count].to_vec().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        warn!(%address, %error, "desktop VNC read failed");
                        break;
                    }
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
                    Message::Binary(bytes) => {
                        if tcp_writer.write_all(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Message::Text(text) => {
                        if tcp_writer.write_all(text.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }

    debug!(
        %address,
        duration_ms = connected_at.elapsed().as_millis(),
        "desktop VNC bridge closed"
    );
}

fn vnc_address(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    }
}
