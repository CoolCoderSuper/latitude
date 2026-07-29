use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use tokio::sync::{Mutex, RwLock, mpsc};
use tracing::{debug, warn};
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
        setting_engine::SettingEngine,
    },
    data_channel::{RTCDataChannel, data_channel_state::RTCDataChannelState},
    ice::network_type::NetworkType,
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::{TrackLocal, track_local_static_sample::TrackLocalStaticSample},
};

use crate::desktop::{
    DesktopTarget, NativeDesktopCommand, NativeInputState, apply_native_desktop_command,
};

const CONTROL_CHANNEL_LABEL: &str = "latitude-control";
const POINTER_CHANNEL_LABEL: &str = "latitude-pointer";
const ICE_GATHER_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) struct NativePeerSession {
    pub(super) peer: Arc<RTCPeerConnection>,
    pub(super) track: Arc<TrackLocalStaticSample>,
    pub(super) state_rx: mpsc::UnboundedReceiver<RTCPeerConnectionState>,
    pub(super) control_channel: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    pub(super) input_state: Arc<Mutex<NativeInputState>>,
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

    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: webrtc::api::media_engine::MIME_TYPE_H264.to_owned(),
            clock_rate: 90_000,
            sdp_fmtp_line: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                .to_owned(),
            ..Default::default()
        },
        "desktop-video".to_owned(),
        "latitude".to_owned(),
    ));
    let sender = peer
        .add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>)
        .await
        .context("WebRTC desktop video track could not be added")?;
    tokio::spawn(async move { while sender.read_rtcp().await.is_ok() {} });

    let input_state = Arc::new(Mutex::new(NativeInputState::default()));
    let control_channel = Arc::new(RwLock::new(None));
    install_data_channel_handler(
        &peer,
        Arc::clone(&control_channel),
        Arc::clone(&input_state),
        view_only,
    );

    let (state_tx, state_rx) = mpsc::unbounded_channel();
    peer.on_peer_connection_state_change(Box::new(move |state| {
        let state_tx = state_tx.clone();
        Box::pin(async move {
            let _ = state_tx.send(state);
        })
    }));

    peer.set_remote_description(
        RTCSessionDescription::offer(offer_sdp).context("WebRTC offer SDP was invalid")?,
    )
    .await
    .context("WebRTC offer could not be applied")?;
    let answer = peer
        .create_answer(None)
        .await
        .context("WebRTC answer could not be created")?;
    let mut gathering_complete = peer.gathering_complete_promise().await;
    peer.set_local_description(answer)
        .await
        .context("WebRTC local answer could not be applied")?;
    let _ = tokio::time::timeout(ICE_GATHER_TIMEOUT, gathering_complete.recv()).await;

    Ok(NativePeerSession {
        peer,
        track,
        state_rx,
        control_channel,
        input_state,
    })
}

fn install_data_channel_handler(
    peer: &RTCPeerConnection,
    control_channel: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    input_state: Arc<Mutex<NativeInputState>>,
    view_only: bool,
) {
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

        if is_control {
            let channel_for_open = Arc::clone(&channel);
            let control_channel_for_open = Arc::clone(&control_channel);
            channel.on_open(Box::new(move || {
                let channel = Arc::clone(&channel_for_open);
                let control_channel = Arc::clone(&control_channel_for_open);
                Box::pin(async move {
                    *control_channel.write().await = Some(channel);
                })
            }));
        }

        let input_state_for_message = Arc::clone(&input_state);
        channel.on_message(Box::new(move |message| {
            let input_state = Arc::clone(&input_state_for_message);
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
                if view_only && !matches!(&command, NativeDesktopCommand::Refresh) {
                    return;
                }
                if is_pointer && !matches!(&command, NativeDesktopCommand::PointerMove { .. }) {
                    return;
                }
                let mut state = input_state.lock().await;
                if let Err(error) = apply_native_desktop_command(command, &mut state) {
                    warn!(%error, "native WebRTC desktop input failed");
                }
            })
        }));

        if is_control {
            let input_state_for_close = Arc::clone(&input_state);
            let control_channel_for_close = Arc::clone(&control_channel);
            channel.on_close(Box::new(move || {
                let input_state = Arc::clone(&input_state_for_close);
                let control_channel = Arc::clone(&control_channel_for_close);
                Box::pin(async move {
                    *control_channel.write().await = None;
                    release_native_input(&input_state).await;
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

pub(super) async fn release_native_input(input_state: &Mutex<NativeInputState>) {
    let mut state = input_state.lock().await;
    if state.buttons != 0 {
        let x = state.x;
        let y = state.y;
        let _ = apply_native_desktop_command(
            NativeDesktopCommand::Pointer { x, y, buttons: 0 },
            &mut state,
        );
    }
    if !state.keys.is_empty() {
        let _ = apply_native_desktop_command(NativeDesktopCommand::ReleaseKeys, &mut state);
    }
}
