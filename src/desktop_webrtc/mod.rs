mod ice;
mod peer;
mod video;

use std::{
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use axum::extract::ws::{Message, WebSocket};
use serde::Deserialize;
use tracing::{debug, info, warn};
use webrtc::{
    ice_transport::ice_candidate::RTCIceCandidateInit,
    peer_connection::{RTCPeerConnection, peer_connection_state::RTCPeerConnectionState},
};

use self::{
    ice::{log_candidates, rewrite_mdns_candidate, rewrite_mdns_candidates},
    peer::{create_session, release_native_input},
    video::start_video_pipeline,
};
use crate::desktop::{DesktopProtocol, DesktopTarget, native_desktop_geometry};

const SIGNAL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NativeWebRtcSignal {
    Offer { sdp: String },
    Candidate { candidate: RTCIceCandidateInit },
}

pub(crate) async fn native_desktop_websocket_session(
    mut socket: WebSocket,
    target: DesktopTarget,
    view_only: bool,
    peer_ip: Option<IpAddr>,
) {
    if let Err(error) = run_native_desktop_session(&mut socket, target, view_only, peer_ip).await {
        warn!(%error, "native WebRTC desktop session failed");
        let message = serde_json::json!({
            "type": "error",
            "message": error.to_string(),
        });
        let _ = socket.send(Message::Text(message.to_string().into())).await;
    }
}

async fn run_native_desktop_session(
    socket: &mut WebSocket,
    target: DesktopTarget,
    view_only: bool,
    peer_ip: Option<IpAddr>,
) -> Result<()> {
    let connected_at = Instant::now();
    let geometry = native_desktop_geometry().map_err(|error| anyhow!(error.to_string()))?;
    let hello = serde_json::json!({
        "type": "hello",
        "protocol": DesktopProtocol::LatitudeNative,
        "transport": "webrtc",
        "codec": "h264",
        "origin_x": geometry.origin_x,
        "origin_y": geometry.origin_y,
        "width": geometry.width,
        "height": geometry.height,
        "view_only": view_only,
        "ice_servers": target.native_ice_servers,
    });
    socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .context("WebRTC hello could not be sent")?;

    let offer = receive_offer(socket).await?;
    log_candidates("offer", &offer);
    let (offer, rewritten_candidates) = rewrite_mdns_candidates(&offer, peer_ip);
    if rewritten_candidates > 0 {
        info!(
            rewritten_candidates,
            "resolved browser mDNS candidates from the authenticated WebSocket peer"
        );
    }
    let mut peer_session = create_session(&target, view_only, offer).await?;
    let answer = peer_session
        .peer
        .local_description()
        .await
        .context("WebRTC answer was not available")?;
    let answer_message = serde_json::json!({
        "type": "answer",
        "sdp": answer.sdp,
    });
    log_candidates("answer", &answer.sdp);
    socket
        .send(Message::Text(answer_message.to_string().into()))
        .await
        .context("WebRTC answer could not be sent")?;

    let mut pipeline = None;
    let mut candidates_open = true;
    loop {
        tokio::select! {
            state = peer_session.state_rx.recv() => {
                let Some(state) = state else {
                    break;
                };
                debug!(?state, "native desktop WebRTC state changed");
                match state {
                    RTCPeerConnectionState::Connected if pipeline.is_none() => {
                        pipeline = Some(start_video_pipeline(
                            Arc::clone(&peer_session.track),
                            Arc::clone(&peer_session.control_channel),
                            target.native_max_fps,
                            target.native_bitrate_kbps,
                        )?);
                    }
                    RTCPeerConnectionState::Disconnected
                    | RTCPeerConnectionState::Failed
                    | RTCPeerConnectionState::Closed => break,
                    _ => {}
                }
            }
            candidate = peer_session.candidate_rx.recv(), if candidates_open => {
                if let Some(candidate) = candidate {
                    let message = serde_json::json!({
                        "type": "candidate",
                        "candidate": candidate,
                    });
                    socket
                        .send(Message::Text(message.to_string().into()))
                        .await
                        .context("WebRTC ICE candidate could not be sent")?;
                } else {
                    candidates_open = false;
                }
            }
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Text(text))) => {
                        receive_candidate(&peer_session.peer, text.as_bytes(), peer_ip).await?;
                    }
                    Some(Ok(Message::Ping(_)))
                    | Some(Ok(Message::Pong(_)))
                    | Some(Ok(Message::Binary(_))) => {}
                }
            }
        }
    }

    if let Some(pipeline) = pipeline {
        pipeline.stop().await;
    }
    peer_session.peer.close().await.ok();
    release_native_input(&peer_session.input_state).await;
    debug!(
        duration_ms = connected_at.elapsed().as_millis(),
        "native WebRTC desktop bridge closed"
    );
    Ok(())
}

async fn receive_offer(socket: &mut WebSocket) -> Result<String> {
    let message = tokio::time::timeout(SIGNAL_TIMEOUT, async {
        loop {
            match socket.recv().await {
                Some(Ok(Message::Text(text))) => break Ok(text),
                Some(Ok(Message::Close(_))) | None => {
                    break Err(anyhow!(
                        "WebRTC signaling socket closed before an offer arrived"
                    ));
                }
                Some(Err(error)) => break Err(anyhow!(error)),
                Some(Ok(Message::Ping(_)))
                | Some(Ok(Message::Pong(_)))
                | Some(Ok(Message::Binary(_))) => {}
            }
        }
    })
    .await
    .context("timed out waiting for a WebRTC offer")??;

    match serde_json::from_slice::<NativeWebRtcSignal>(message.as_bytes())
        .context("invalid WebRTC signaling message")?
    {
        NativeWebRtcSignal::Offer { sdp } if !sdp.trim().is_empty() => Ok(sdp),
        NativeWebRtcSignal::Offer { .. } => Err(anyhow!("WebRTC offer SDP was empty")),
        NativeWebRtcSignal::Candidate { .. } => {
            Err(anyhow!("received an ICE candidate before the WebRTC offer"))
        }
    }
}

async fn receive_candidate(
    peer: &RTCPeerConnection,
    message: &[u8],
    peer_ip: Option<IpAddr>,
) -> Result<()> {
    let signal = serde_json::from_slice::<NativeWebRtcSignal>(message)
        .context("invalid WebRTC signaling message")?;
    let NativeWebRtcSignal::Candidate { mut candidate } = signal else {
        return Err(anyhow!("received an unexpected WebRTC signaling message"));
    };
    let (rewritten, changed) = rewrite_mdns_candidate(&candidate.candidate, peer_ip);
    candidate.candidate = rewritten;
    if changed {
        info!("resolved a trickled browser mDNS candidate from the authenticated WebSocket peer");
    }
    peer.add_ice_candidate(candidate)
        .await
        .context("WebRTC ICE candidate could not be applied")
}
