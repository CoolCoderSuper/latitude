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
    peer::{create_session, send_control_message},
    video::{NativeVideoSettings, h264_profile_level_id, start_video_pipeline},
};
use crate::desktop::{
    DesktopSessionConfig, detect_desktop_screens, fit_native_desktop_geometry,
    native_desktop_geometry, native_input_controller, scale_native_desktop_screens,
};

const SIGNAL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NativeWebRtcSignal {
    Offer { sdp: String },
    Candidate { candidate: RTCIceCandidateInit },
}

pub(crate) async fn desktop_websocket_session(
    mut socket: WebSocket,
    session_config: DesktopSessionConfig,
    view_only: bool,
    peer_ip: Option<IpAddr>,
) {
    if let Err(error) = run_desktop_session(&mut socket, session_config, view_only, peer_ip).await {
        warn!(%error, "WebRTC desktop session failed");
        let message = serde_json::json!({
            "type": "error",
            "message": error.to_string(),
        });
        let _ = socket.send(Message::Text(message.to_string().into())).await;
    }
}

async fn run_desktop_session(
    socket: &mut WebSocket,
    session_config: DesktopSessionConfig,
    view_only: bool,
    peer_ip: Option<IpAddr>,
) -> Result<()> {
    let connected_at = Instant::now();
    let source_geometry = native_desktop_geometry().map_err(|error| anyhow!(error.to_string()))?;
    let geometry = fit_native_desktop_geometry(
        source_geometry,
        session_config.max_width,
        session_config.max_height,
    );
    let screens = scale_native_desktop_screens(detect_desktop_screens(), source_geometry, geometry);
    let h264_profile_level_id = h264_profile_level_id(
        session_config.max_width,
        session_config.max_height,
        session_config.max_fps,
        session_config.bitrate_kbps,
    );
    let hello = serde_json::json!({
        "type": "hello",
        "transport": "webrtc",
        "codec": "h264",
        "origin_x": geometry.origin_x,
        "origin_y": geometry.origin_y,
        "width": geometry.width,
        "height": geometry.height,
        "source_width": source_geometry.width,
        "source_height": source_geometry.height,
        "screens": screens,
        "view_only": view_only,
        "ice_servers": session_config.ice_servers,
        "h264_profile_level_id": h264_profile_level_id,
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
    let mut peer_session = create_session(&session_config, view_only, offer).await?;
    let mut pipeline = None;
    let session_result: Result<()> = async {
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

        let mut candidates_open = true;
        let mut controller_events_open = true;
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
                                NativeVideoSettings::new(
                                    session_config.max_fps,
                                    session_config.bitrate_kbps,
                                    session_config.max_width,
                                    session_config.max_height,
                                ),
                            ).await?);
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
                controller_state = peer_session.controller_state_rx.changed(), if controller_events_open => {
                    if controller_state.is_err() {
                        controller_events_open = false;
                        continue;
                    }
                    let state = *peer_session.controller_state_rx.borrow_and_update();
                    send_control_message(
                        &peer_session.control_channel,
                        serde_json::json!({
                            "type": "control",
                            "state": state.as_str(),
                        }),
                    )
                    .await;
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

        Ok(())
    }
    .await;

    peer_session.peer.close().await.ok();
    if let Some(pipeline) = pipeline.take() {
        pipeline.stop().await;
    }
    native_input_controller()
        .unregister(peer_session.session_id)
        .await;
    debug!(
        duration_ms = connected_at.elapsed().as_millis(),
        "native WebRTC desktop bridge closed"
    );
    session_result
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
