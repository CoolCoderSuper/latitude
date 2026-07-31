use anyhow::{Context, Result};
use axum::extract::ws::Message as AxumMessage;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

pub(crate) async fn forward_websocket<S>(
    browser: &mut axum::extract::ws::WebSocket,
    upstream: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        tokio::select! {
            message = browser.recv() => {
                let Some(message) = message else {
                    let _ = upstream.close(None).await;
                    return Ok(());
                };
                let message = message.context("browser WebSocket failed")?;
                let closes = matches!(message, AxumMessage::Close(_));
                upstream.send(to_tungstenite(message)).await.context("browser message could not be forwarded")?;
                if closes {
                    return Ok(());
                }
            }
            message = upstream.next() => {
                let Some(message) = message else {
                    return Ok(());
                };
                let message = message.context("upstream WebSocket failed")?;
                if let Some(message) = to_axum(message) {
                    let closes = matches!(message, AxumMessage::Close(_));
                    browser.send(message).await.context("upstream message could not be forwarded")?;
                    if closes {
                        return Ok(());
                    }
                }
            }
        }
    }
}

pub(crate) fn to_tungstenite(message: AxumMessage) -> TungsteniteMessage {
    match message {
        AxumMessage::Text(text) => TungsteniteMessage::Text(text.to_string().into()),
        AxumMessage::Binary(bytes) => TungsteniteMessage::Binary(bytes.to_vec().into()),
        AxumMessage::Ping(bytes) => TungsteniteMessage::Ping(bytes.to_vec().into()),
        AxumMessage::Pong(bytes) => TungsteniteMessage::Pong(bytes.to_vec().into()),
        AxumMessage::Close(_) => TungsteniteMessage::Close(None),
    }
}

pub(crate) fn to_axum(message: TungsteniteMessage) -> Option<AxumMessage> {
    match message {
        TungsteniteMessage::Text(text) => Some(AxumMessage::Text(text.to_string().into())),
        TungsteniteMessage::Binary(bytes) => Some(AxumMessage::Binary(bytes.to_vec().into())),
        TungsteniteMessage::Ping(bytes) => Some(AxumMessage::Ping(bytes.to_vec().into())),
        TungsteniteMessage::Pong(bytes) => Some(AxumMessage::Pong(bytes.to_vec().into())),
        TungsteniteMessage::Close(_) => Some(AxumMessage::Close(None)),
        TungsteniteMessage::Frame(_) => None,
    }
}
