use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use axum::{
    Router,
    body::Body,
    extract::{State, ws::WebSocketUpgrade},
    http::{HeaderMap, Response, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::RwLock};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest},
};
use tracing::{debug, info, warn};

use crate::websocket_bridge::forward_websocket;

use super::DesktopSessionConfig;

const SESSION_HOST_DESKTOP_PATH: &str = "/desktop";
const SESSION_HOST_HEALTH_PATH: &str = "/health";

#[derive(Clone, Debug)]
struct NativeSessionEndpoint {
    address: SocketAddr,
    token: String,
}

#[derive(Clone, Default)]
pub(crate) struct NativeSessionBridge {
    endpoint: Arc<RwLock<Option<NativeSessionEndpoint>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeSessionRequest {
    session_config: DesktopSessionConfig,
    view_only: bool,
    peer_ip: Option<IpAddr>,
}

#[derive(Clone)]
struct SessionHostState {
    token: Arc<str>,
}

impl NativeSessionBridge {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn set_endpoint(&self, address: SocketAddr, token: impl Into<String>) {
        *self.endpoint.write().await = Some(NativeSessionEndpoint {
            address,
            token: token.into(),
        });
    }

    pub(crate) async fn clear_endpoint(&self) {
        self.endpoint.write().await.take();
    }

    pub(crate) async fn is_available(&self) -> bool {
        self.endpoint.read().await.is_some()
    }

    pub(crate) async fn proxy(
        &self,
        mut browser: axum::extract::ws::WebSocket,
        session_config: DesktopSessionConfig,
        view_only: bool,
        peer_ip: Option<IpAddr>,
    ) {
        if let Err(error) = self
            .run_proxy(&mut browser, session_config, view_only, peer_ip)
            .await
        {
            warn!(%error, "native desktop session-host proxy failed");
            let message = serde_json::json!({
                "type": "error",
                "message": error.to_string(),
            });
            let _ = browser
                .send(axum::extract::ws::Message::Text(message.to_string().into()))
                .await;
        }
    }

    async fn run_proxy(
        &self,
        browser: &mut axum::extract::ws::WebSocket,
        session_config: DesktopSessionConfig,
        view_only: bool,
        peer_ip: Option<IpAddr>,
    ) -> Result<()> {
        let endpoint = self
            .endpoint
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("no interactive Windows desktop session is available"))?;
        let url = format!("ws://{}{}", endpoint.address, SESSION_HOST_DESKTOP_PATH);
        let mut request = url
            .into_client_request()
            .context("session-host WebSocket URL was invalid")?;
        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {}", endpoint.token)
                .parse()
                .context("session-host authorization header was invalid")?,
        );
        let (mut worker, _) = connect_async(request)
            .await
            .context("interactive Windows desktop session host is unavailable")?;
        worker
            .send(tungstenite::Message::Text(
                serde_json::to_string(&NativeSessionRequest {
                    session_config,
                    view_only,
                    peer_ip,
                })?
                .into(),
            ))
            .await
            .context("desktop session parameters could not be sent")?;

        forward_websocket(browser, &mut worker).await?;
        debug!("native desktop session-host proxy closed");
        Ok(())
    }
}

pub(crate) async fn run_native_session_host(address: SocketAddr, token: String) -> Result<()> {
    if !address.ip().is_loopback() {
        return Err(anyhow!(
            "the native desktop session host must bind to a loopback address"
        ));
    }
    if token.len() < 32 {
        return Err(anyhow!(
            "the native desktop session-host token must contain at least 32 characters"
        ));
    }

    let state = SessionHostState {
        token: Arc::from(token),
    };
    let router = Router::new()
        .route(SESSION_HOST_HEALTH_PATH, get(session_host_health))
        .route(SESSION_HOST_DESKTOP_PATH, get(session_host_desktop))
        .with_state(state);
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("native desktop session host could not bind {address}"))?;
    info!(
        bind = %listener.local_addr()?,
        "privileged native desktop session host listening"
    );
    axum::serve(listener, router).await?;
    Ok(())
}

async fn session_host_health(
    State(state): State<SessionHostState>,
    headers: HeaderMap,
) -> Response<Body> {
    if !session_host_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn session_host_desktop(
    State(state): State<SessionHostState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    if !session_host_is_authenticated(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    ws.on_upgrade(|mut socket| async move {
        let request = match socket.recv().await {
            Some(Ok(axum::extract::ws::Message::Text(message))) => {
                serde_json::from_slice::<NativeSessionRequest>(message.as_bytes())
            }
            _ => return,
        };
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                let message = serde_json::json!({
                    "type": "error",
                    "message": format!("invalid desktop session parameters: {error}"),
                });
                let _ = socket
                    .send(axum::extract::ws::Message::Text(message.to_string().into()))
                    .await;
                return;
            }
        };

        crate::desktop_webrtc::desktop_websocket_session(
            socket,
            request.session_config,
            request.view_only,
            request.peer_ip,
        )
        .await;
    })
}

fn session_host_is_authenticated(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|provided| provided == expected)
}
