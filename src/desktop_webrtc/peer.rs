use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{RwLock, mpsc, watch};
use tracing::{debug, warn};
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
        setting_engine::SettingEngine,
    },
    data_channel::{RTCDataChannel, data_channel_state::RTCDataChannelState},
    ice::network_type::NetworkType,
    ice_transport::ice_candidate::RTCIceCandidateInit,
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtcp::{
        packet::Packet,
        payload_feedbacks::{
            full_intra_request::FullIntraRequest, picture_loss_indication::PictureLossIndication,
            slice_loss_indication::SliceLossIndication,
        },
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::{TrackLocal, track_local_static_sample::TrackLocalStaticSample},
};

use crate::desktop::{
    DesktopTarget, NativeControllerLeaseState, NativeDesktopCommand, NativeInputController,
    native_input_controller,
};

use super::video::{NativeVideoSettings, h264_profile_level_id, request_video_keyframe};

const CONTROL_CHANNEL_LABEL: &str = "latitude-control";
const POINTER_CHANNEL_LABEL: &str = "latitude-pointer";

#[derive(Clone)]
struct PointerMoveDispatcher {
    latest: watch::Sender<Option<(f64, f64)>>,
}

impl PointerMoveDispatcher {
    fn new(session_id: u64, view_only: bool) -> Self {
        let (latest, mut pending) = watch::channel(None);
        tokio::spawn(async move {
            while pending.changed().await.is_ok() {
                let Some((x, y)) = *pending.borrow_and_update() else {
                    continue;
                };
                if view_only {
                    continue;
                }
                if let Err(error) = native_input_controller()
                    .apply(session_id, NativeDesktopCommand::PointerMove { x, y })
                    .await
                {
                    warn!(%error, "native WebRTC pointer move failed");
                }
            }
        });
        Self { latest }
    }

    fn submit(&self, x: f64, y: f64) {
        self.latest.send_replace(Some((x, y)));
    }
}

pub(super) struct NativePeerSession {
    pub(super) peer: Arc<RTCPeerConnection>,
    pub(super) track: Arc<TrackLocalStaticSample>,
    pub(super) state_rx: mpsc::UnboundedReceiver<RTCPeerConnectionState>,
    pub(super) candidate_rx: mpsc::UnboundedReceiver<RTCIceCandidateInit>,
    pub(super) control_channel: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    pub(super) controller_state_rx: watch::Receiver<NativeControllerLeaseState>,
    pub(super) session_id: u64,
}

pub(super) async fn create_session(
    target: &DesktopTarget,
    view_only: bool,
    offer_sdp: String,
) -> Result<NativePeerSession> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .context("WebRTC codecs could not be registered")?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .context("WebRTC interceptors could not be registered")?;
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_network_types(vec![NetworkType::Udp4]);
    setting_engine.set_include_loopback_candidate(true);
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_setting_engine(setting_engine)
        .build();
    let ice_servers = target
        .native_ice_servers
        .iter()
        .map(|server| RTCIceServer {
            urls: server.urls.clone(),
            username: server.username.clone(),
            credential: server.credential.clone(),
        })
        .collect();
    let peer = Arc::new(
        api.new_peer_connection(RTCConfiguration {
            ice_servers,
            ..Default::default()
        })
        .await
        .context("WebRTC peer connection could not be created")?,
    );

    let video_profile_level_id = h264_profile_level_id(
        target.native_max_width,
        target.native_max_height,
        target.native_max_fps,
        target.native_bitrate_kbps,
    );
    let video_settings = NativeVideoSettings::new(
        target.native_max_fps,
        target.native_bitrate_kbps,
        target.native_max_width,
        target.native_max_height,
    );
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: webrtc::api::media_engine::MIME_TYPE_H264.to_owned(),
            clock_rate: 90_000,
            sdp_fmtp_line: format!(
                "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id={video_profile_level_id}"
            ),
            ..Default::default()
        },
        "desktop-video".to_owned(),
        "latitude".to_owned(),
    ));
    let sender = peer
        .add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>)
        .await
        .context("WebRTC desktop video track could not be added")?;
    tokio::spawn(async move {
        while let Ok((packets, _)) = sender.read_rtcp().await {
            if packets
                .iter()
                .any(|packet| requests_keyframe(packet.as_ref()))
            {
                request_video_keyframe(video_settings).await;
                debug!("native WebRTC keyframe requested by RTCP feedback");
            }
        }
    });

    let controller: &NativeInputController = native_input_controller();
    let session_id = controller.next_session_id();
    let controller_state_rx = controller.subscribe(session_id, !view_only).await;
    let control_channel = Arc::new(RwLock::new(None));
    install_data_channel_handler(
        &peer,
        Arc::clone(&control_channel),
        session_id,
        view_only,
        video_settings,
    );

    let (state_tx, state_rx) = mpsc::unbounded_channel();
    peer.on_peer_connection_state_change(Box::new(move |state| {
        let state_tx = state_tx.clone();
        Box::pin(async move {
            let _ = state_tx.send(state);
        })
    }));
    let (candidate_tx, candidate_rx) = mpsc::unbounded_channel();
    peer.on_ice_candidate(Box::new(move |candidate| {
        let candidate_tx = candidate_tx.clone();
        Box::pin(async move {
            let Some(candidate) = candidate else {
                return;
            };
            match candidate.to_json() {
                Ok(candidate) => {
                    let _ = candidate_tx.send(candidate);
                }
                Err(error) => {
                    warn!(%error, "native WebRTC ICE candidate could not be serialized");
                }
            }
        })
    }));

    let setup_result: Result<()> = async {
        peer.set_remote_description(
            RTCSessionDescription::offer(offer_sdp).context("WebRTC offer SDP was invalid")?,
        )
        .await
        .context("WebRTC offer could not be applied")?;
        let answer = peer
            .create_answer(None)
            .await
            .context("WebRTC answer could not be created")?;
        peer.set_local_description(answer)
            .await
            .context("WebRTC local answer could not be applied")
    }
    .await;
    if let Err(error) = setup_result {
        peer.close().await.ok();
        controller.unregister(session_id).await;
        return Err(error);
    }

    Ok(NativePeerSession {
        peer,
        track,
        state_rx,
        candidate_rx,
        control_channel,
        controller_state_rx,
        session_id,
    })
}

fn install_data_channel_handler(
    peer: &RTCPeerConnection,
    control_channel: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    session_id: u64,
    view_only: bool,
    video_settings: NativeVideoSettings,
) {
    let pointer_moves = PointerMoveDispatcher::new(session_id, view_only);
    peer.on_data_channel(Box::new(move |channel| {
        let is_control = channel.label() == CONTROL_CHANNEL_LABEL;
        let is_pointer = channel.label() == POINTER_CHANNEL_LABEL;
        if !is_control && !is_pointer {
            return Box::pin(async {});
        }
        debug!(
            label = channel.label(),
            "native WebRTC data channel accepted"
        );

        let pointer_moves_for_message = pointer_moves.clone();
        if is_control {
            let channel_for_open = Arc::clone(&channel);
            let control_channel_for_open = Arc::clone(&control_channel);
            channel.on_open(Box::new(move || {
                let channel = Arc::clone(&channel_for_open);
                let control_channel = Arc::clone(&control_channel_for_open);
                Box::pin(async move {
                    *control_channel.write().await = Some(channel);
                    native_input_controller().activate(session_id).await;
                })
            }));
        }

        channel.on_message(Box::new(move |message| {
            let pointer_moves = pointer_moves_for_message.clone();
            Box::pin(async move {
                if !message.is_string {
                    return;
                }
                let command =
                    match serde_json::from_slice::<NativeDesktopCommand>(message.data.as_ref()) {
                        Ok(command) => command,
                        Err(error) => {
                            debug!(%error, "native WebRTC desktop command was rejected");
                            return;
                        }
                    };
                if let NativeDesktopCommand::PointerMove { x, y } = command {
                    pointer_moves.submit(x, y);
                    return;
                }
                if is_pointer {
                    return;
                }
                if matches!(&command, NativeDesktopCommand::Refresh) {
                    request_video_keyframe(video_settings).await;
                    return;
                }
                if view_only {
                    return;
                }
                if let Err(error) = native_input_controller().apply(session_id, command).await {
                    warn!(%error, "native WebRTC desktop input failed");
                }
            })
        }));

        if is_control {
            let channel_for_close = Arc::clone(&channel);
            let control_channel_for_close = Arc::clone(&control_channel);
            channel.on_close(Box::new(move || {
                let channel = Arc::clone(&channel_for_close);
                let control_channel = Arc::clone(&control_channel_for_close);
                Box::pin(async move {
                    let should_release = {
                        let mut current = control_channel.write().await;
                        if current
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, &channel))
                        {
                            *current = None;
                            true
                        } else {
                            false
                        }
                    };
                    if should_release {
                        native_input_controller().unregister(session_id).await;
                    }
                })
            }));
        }

        Box::pin(async {})
    }));
}

pub(super) async fn send_control_message(
    control_channel: &RwLock<Option<Arc<RTCDataChannel>>>,
    message: serde_json::Value,
) {
    let channel = control_channel.read().await.clone();
    if let Some(channel) = channel
        && channel.ready_state() == RTCDataChannelState::Open
    {
        let _ = channel.send_text(message.to_string()).await;
    }
}

fn requests_keyframe(packet: &(dyn Packet + Send + Sync)) -> bool {
    packet
        .as_any()
        .downcast_ref::<PictureLossIndication>()
        .is_some()
        || packet.as_any().downcast_ref::<FullIntraRequest>().is_some()
        || packet
            .as_any()
            .downcast_ref::<SliceLossIndication>()
            .is_some()
}

#[cfg(test)]
mod tests {
    use webrtc::rtcp::{
        payload_feedbacks::{
            full_intra_request::FullIntraRequest, picture_loss_indication::PictureLossIndication,
        },
        receiver_report::ReceiverReport,
    };

    use super::requests_keyframe;

    #[test]
    fn recognizes_rtcp_keyframe_feedback() {
        assert!(requests_keyframe(&PictureLossIndication::default()));
        assert!(requests_keyframe(&FullIntraRequest::default()));
        assert!(!requests_keyframe(&ReceiverReport::default()));
    }
}
