use std::{
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use axum::extract::ws::{Message, WebSocket};
use openh264::{
    OpenH264API,
    encoder::{
        BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Profile,
        RateControlMode, UsageType, VuiConfig,
    },
    formats::YUVSource,
};
use serde::Deserialize;
use tokio::{
    sync::{Mutex, RwLock, mpsc, watch},
    task::JoinHandle,
};
use tracing::{debug, info, warn};
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
        setting_engine::SettingEngine,
    },
    data_channel::{RTCDataChannel, data_channel_state::RTCDataChannelState},
    ice::network_type::NetworkType,
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    media::Sample,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::{TrackLocal, track_local_static_sample::TrackLocalStaticSample},
};
use yuvutils_rs::{
    BufferStoreMut, YuvConversionMode, YuvPlanarImageMut, YuvRange, YuvStandardMatrix,
    bgra_to_yuv420,
};

use crate::desktop::{
    DesktopProtocol, DesktopTarget, NativeDesktopCapture, NativeDesktopCommand,
    NativeDesktopCursor, NativeDesktopFrame, NativeDesktopGeometry, NativeInputState,
    apply_native_desktop_command, native_desktop_geometry,
};

const CONTROL_CHANNEL_LABEL: &str = "latitude-control";
const POINTER_CHANNEL_LABEL: &str = "latitude-pointer";
const SIGNAL_TIMEOUT: Duration = Duration::from_secs(30);
const ICE_GATHER_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NativeWebRtcSignal {
    Offer { sdp: String },
}

struct NativePeerSession {
    peer: Arc<RTCPeerConnection>,
    track: Arc<TrackLocalStaticSample>,
    state_rx: mpsc::UnboundedReceiver<RTCPeerConnectionState>,
    control_channel: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    input_state: Arc<Mutex<NativeInputState>>,
}

struct EncodedDesktopFrame {
    geometry: NativeDesktopGeometry,
    cursor: NativeDesktopCursor,
    h264: Vec<u8>,
}

enum CapturedDesktopEvent {
    Frame(NativeDesktopFrame),
    Error(String),
}

struct DesktopYuvBuffer {
    width: usize,
    height: usize,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

impl DesktopYuvBuffer {
    fn new(width: usize, height: usize) -> Self {
        let chroma_len = width * height / 4;
        Self {
            width,
            height,
            y: vec![0; width * height],
            u: vec![0; chroma_len],
            v: vec![0; chroma_len],
        }
    }

    fn resize(&mut self, width: usize, height: usize) {
        if self.dimensions() != (width, height) {
            *self = Self::new(width, height);
        }
    }

    fn read_bgra(&mut self, bgra: &[u8]) -> Result<(), yuvutils_rs::YuvError> {
        let mut image = YuvPlanarImageMut {
            y_plane: BufferStoreMut::Borrowed(&mut self.y),
            y_stride: self.width as u32,
            u_plane: BufferStoreMut::Borrowed(&mut self.u),
            u_stride: (self.width / 2) as u32,
            v_plane: BufferStoreMut::Borrowed(&mut self.v),
            v_stride: (self.width / 2) as u32,
            width: self.width as u32,
            height: self.height as u32,
        };
        bgra_to_yuv420(
            &mut image,
            bgra,
            (self.width * 4) as u32,
            YuvRange::Full,
            YuvStandardMatrix::Bt709,
            YuvConversionMode::Fast,
        )
    }
}

impl YUVSource for DesktopYuvBuffer {
    fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn strides(&self) -> (usize, usize, usize) {
        (self.width, self.width / 2, self.width / 2)
    }

    fn y(&self) -> &[u8] {
        &self.y
    }

    fn u(&self) -> &[u8] {
        &self.u
    }

    fn v(&self) -> &[u8] {
        &self.v
    }
}

struct NativeVideoPipeline {
    stop_tx: watch::Sender<bool>,
    capture: JoinHandle<()>,
    encoder: JoinHandle<()>,
    writer: JoinHandle<()>,
}

impl NativeVideoPipeline {
    async fn stop(self) {
        let _ = self.stop_tx.send(true);
        let _ = self.capture.await;
        let _ = self.encoder.await;
        let _ = self.writer.await;
    }
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
    log_ice_candidates("offer", &offer);
    let (offer, rewritten_candidates) = rewrite_mdns_candidates(&offer, peer_ip);
    if rewritten_candidates > 0 {
        info!(
            rewritten_candidates,
            "resolved browser mDNS candidates from the authenticated WebSocket peer"
        );
    }
    let mut peer_session = create_peer_session(&target, view_only, offer).await?;
    let answer = peer_session
        .peer
        .local_description()
        .await
        .context("WebRTC answer was not available after ICE gathering")?;
    let answer_message = serde_json::json!({
        "type": "answer",
        "sdp": answer.sdp,
    });
    log_ice_candidates("answer", &answer.sdp);
    socket
        .send(Message::Text(answer_message.to_string().into()))
        .await
        .context("WebRTC answer could not be sent")?;

    let mut pipeline = None;
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
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Ping(_)))
                    | Some(Ok(Message::Pong(_)))
                    | Some(Ok(Message::Text(_)))
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
    }
}

async fn create_peer_session(
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

fn start_video_pipeline(
    track: Arc<TrackLocalStaticSample>,
    control_channel: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    fps: u16,
    bitrate_kbps: u32,
) -> Result<NativeVideoPipeline> {
    let (capture_tx, capture_rx) = watch::channel::<Option<Arc<CapturedDesktopEvent>>>(None);
    let (frame_tx, mut frame_rx) = mpsc::channel::<Result<EncodedDesktopFrame, String>>(1);
    let (stop_tx, stop_rx) = watch::channel(false);
    let capture_stop_rx = stop_rx.clone();
    let capture = tokio::task::spawn_blocking(move || {
        capture_desktop_frames(capture_tx, capture_stop_rx, fps);
    });
    let runtime = tokio::runtime::Handle::current();
    let encoder = tokio::task::spawn_blocking(move || {
        encode_desktop_frames(runtime, capture_rx, frame_tx, stop_rx, fps, bitrate_kbps);
    });
    let writer = tokio::spawn(async move {
        let duration = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
        let mut last_geometry = None;
        let mut last_cursor = None;
        let mut stats_started = Instant::now();
        let mut sent_frames = 0_u64;
        let mut sent_bytes = 0_u64;
        while let Some(frame) = frame_rx.recv().await {
            let frame = match frame {
                Ok(frame) => frame,
                Err(error) => {
                    send_control_message(
                        &control_channel,
                        serde_json::json!({ "type": "error", "message": error }),
                    )
                    .await;
                    break;
                }
            };
            if last_geometry != Some(frame.geometry) {
                send_control_message(
                    &control_channel,
                    serde_json::json!({
                        "type": "geometry",
                        "origin_x": frame.geometry.origin_x,
                        "origin_y": frame.geometry.origin_y,
                        "width": frame.geometry.width,
                        "height": frame.geometry.height,
                    }),
                )
                .await;
                last_geometry = Some(frame.geometry);
            }
            if last_cursor != Some(frame.cursor) {
                send_control_message(
                    &control_channel,
                    serde_json::json!({ "type": "cursor", "cursor": frame.cursor }),
                )
                .await;
                last_cursor = Some(frame.cursor);
            }
            let encoded_bytes = frame.h264.len() as u64;
            if let Err(error) = track
                .write_sample(&Sample {
                    data: frame.h264.into(),
                    duration,
                    ..Default::default()
                })
                .await
            {
                warn!(%error, "native WebRTC desktop frame could not be sent");
                break;
            }
            sent_frames += 1;
            sent_bytes += encoded_bytes;
            let stats_elapsed = stats_started.elapsed();
            if stats_elapsed >= Duration::from_secs(5) {
                debug!(
                    frames_per_second = sent_frames as f64 / stats_elapsed.as_secs_f64(),
                    payload_kbps = sent_bytes as f64 * 8.0 / stats_elapsed.as_secs_f64() / 1_000.0,
                    "native WebRTC desktop media rate"
                );
                stats_started = Instant::now();
                sent_frames = 0;
                sent_bytes = 0;
            }
        }
    });

    Ok(NativeVideoPipeline {
        stop_tx,
        capture,
        encoder,
        writer,
    })
}

fn capture_desktop_frames(
    frame_tx: watch::Sender<Option<Arc<CapturedDesktopEvent>>>,
    stop_rx: watch::Receiver<bool>,
    fps: u16,
) {
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
    let mut stats_started = Instant::now();
    let mut captured_frames = 0_u64;
    let mut capture_time = Duration::ZERO;
    let mut capture = match NativeDesktopCapture::new() {
        Ok(capture) => capture,
        Err(error) => {
            frame_tx.send_replace(Some(Arc::new(CapturedDesktopEvent::Error(
                error.to_string(),
            ))));
            return;
        }
    };

    loop {
        if *stop_rx.borrow() || frame_tx.is_closed() {
            break;
        }
        let started = Instant::now();
        let frame = match capture.capture() {
            Ok(frame) => frame,
            Err(error) => {
                frame_tx.send_replace(Some(Arc::new(CapturedDesktopEvent::Error(
                    error.to_string(),
                ))));
                break;
            }
        };
        capture_time += started.elapsed();
        captured_frames += 1;
        frame_tx.send_replace(Some(Arc::new(CapturedDesktopEvent::Frame(frame))));

        let stats_elapsed = stats_started.elapsed();
        if stats_elapsed >= Duration::from_secs(5) {
            debug!(
                frames_per_second = captured_frames as f64 / stats_elapsed.as_secs_f64(),
                average_capture_ms = capture_time.as_secs_f64() * 1_000.0 / captured_frames as f64,
                "native WebRTC desktop capture rate"
            );
            stats_started = Instant::now();
            captured_frames = 0;
            capture_time = Duration::ZERO;
        }

        wait_until(started + frame_interval);
    }
}

fn wait_until(deadline: Instant) {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        if remaining > Duration::from_millis(2) {
            std::thread::sleep(remaining - Duration::from_millis(1));
        } else {
            std::hint::spin_loop();
        }
    }
}

fn encode_desktop_frames(
    runtime: tokio::runtime::Handle,
    mut capture_rx: watch::Receiver<Option<Arc<CapturedDesktopEvent>>>,
    frame_tx: mpsc::Sender<Result<EncodedDesktopFrame, String>>,
    stop_rx: watch::Receiver<bool>,
    fps: u16,
    bitrate_kbps: u32,
) {
    let fps = fps.max(1);
    let encoder_config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(bitrate_kbps.saturating_mul(1_000)))
        .max_frame_rate(FrameRate::from_hz(fps as f32))
        .rate_control_mode(RateControlMode::Bitrate)
        .usage_type(UsageType::ScreenContentRealTime)
        .profile(Profile::Baseline)
        .complexity(Complexity::Low)
        .adaptive_quantization(false)
        .background_detection(false)
        .intra_frame_period(IntraFramePeriod::from_num_frames(u32::from(fps) * 2))
        .vui(VuiConfig::srgb())
        .skip_frames(true);
    let mut encoder = match Encoder::with_api_config(OpenH264API::from_source(), encoder_config) {
        Ok(encoder) => encoder,
        Err(error) => {
            let _ = frame_tx.blocking_send(Err(format!(
                "H.264 encoder could not be initialized: {error}"
            )));
            return;
        }
    };
    let mut yuv = None;
    let mut stats_started = Instant::now();
    let mut attempted_frames = 0_u64;
    let mut encoded_frames = 0_u64;
    let mut conversion_time = Duration::ZERO;
    let mut encode_time = Duration::ZERO;

    while runtime.block_on(capture_rx.changed()).is_ok() {
        if *stop_rx.borrow() {
            break;
        }
        let event = capture_rx.borrow_and_update().clone();
        let Some(event) = event else {
            continue;
        };
        let frame = match event.as_ref() {
            CapturedDesktopEvent::Frame(frame) => frame,
            CapturedDesktopEvent::Error(error) => {
                let _ = frame_tx.blocking_send(Err(error.clone()));
                break;
            }
        };
        let dimensions = (
            frame.geometry.width as usize,
            frame.geometry.height as usize,
        );
        let buffer = yuv.get_or_insert_with(|| DesktopYuvBuffer::new(dimensions.0, dimensions.1));
        buffer.resize(dimensions.0, dimensions.1);
        let conversion_started = Instant::now();
        if let Err(error) = buffer.read_bgra(&frame.bgra) {
            let _ = frame_tx.blocking_send(Err(format!(
                "desktop frame could not be converted to YUV: {error}"
            )));
            break;
        }
        conversion_time += conversion_started.elapsed();
        let encode_started = Instant::now();
        let bitstream = match encoder.encode(buffer) {
            Ok(bitstream) => bitstream,
            Err(error) => {
                let _ = frame_tx.blocking_send(Err(format!(
                    "desktop frame could not be encoded as H.264: {error}"
                )));
                break;
            }
        };
        encode_time += encode_started.elapsed();
        let h264 = bitstream.to_vec();
        attempted_frames += 1;
        encoded_frames += u64::from(!h264.is_empty());
        if !h264.is_empty()
            && frame_tx
                .blocking_send(Ok(EncodedDesktopFrame {
                    geometry: frame.geometry,
                    cursor: frame.cursor,
                    h264,
                }))
                .is_err()
        {
            break;
        }

        let stats_elapsed = stats_started.elapsed();
        if stats_elapsed >= Duration::from_secs(5) {
            debug!(
                attempted_frames_per_second = attempted_frames as f64 / stats_elapsed.as_secs_f64(),
                encoded_frames_per_second = encoded_frames as f64 / stats_elapsed.as_secs_f64(),
                average_conversion_ms =
                    conversion_time.as_secs_f64() * 1_000.0 / attempted_frames as f64,
                average_encode_ms = encode_time.as_secs_f64() * 1_000.0 / attempted_frames as f64,
                "native WebRTC desktop producer rate"
            );
            stats_started = Instant::now();
            attempted_frames = 0;
            encoded_frames = 0;
            conversion_time = Duration::ZERO;
            encode_time = Duration::ZERO;
        }
    }
}

async fn send_control_message(
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

async fn release_native_input(input_state: &Mutex<NativeInputState>) {
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

fn log_ice_candidates(description: &str, sdp: &str) {
    let mut host = 0;
    let mut server_reflexive = 0;
    let mut relay = 0;
    let mut peer_reflexive = 0;
    let mut mdns = 0;

    for line in sdp.lines().filter(|line| line.starts_with("a=candidate:")) {
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        if fields
            .get(4)
            .is_some_and(|address| address.ends_with(".local"))
        {
            mdns += 1;
        }
        if let Some(candidate_type) = fields
            .windows(2)
            .find_map(|fields| (fields[0] == "typ").then_some(fields[1]))
        {
            match candidate_type {
                "host" => host += 1,
                "srflx" => server_reflexive += 1,
                "relay" => relay += 1,
                "prflx" => peer_reflexive += 1,
                _ => {}
            }
        }
    }

    info!(
        description,
        host, server_reflexive, relay, peer_reflexive, mdns, "WebRTC ICE candidates gathered"
    );
}

fn rewrite_mdns_candidates(sdp: &str, peer_ip: Option<IpAddr>) -> (String, usize) {
    let Some(IpAddr::V4(peer_ip)) = peer_ip else {
        return (sdp.to_owned(), 0);
    };
    let peer_ip = peer_ip.to_string();
    let mut rewritten = 0;
    let lines = sdp
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split_ascii_whitespace().collect();
            let is_mdns_host = line.starts_with("a=candidate:")
                && fields
                    .get(4)
                    .is_some_and(|address| address.ends_with(".local"))
                && fields
                    .windows(2)
                    .any(|fields| fields[0] == "typ" && fields[1] == "host");
            if is_mdns_host {
                rewritten += 1;
                line.replacen(fields[4], &peer_ip, 1)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n");

    (format!("{lines}\r\n"), rewritten)
}

#[cfg(test)]
mod tests {
    use super::rewrite_mdns_candidates;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn rewrites_only_mdns_host_candidate_addresses() {
        let sdp = concat!(
            "v=0\r\n",
            "a=candidate:host 1 udp 1 browser.local 5000 typ host\r\n",
            "a=candidate:srflx 1 udp 1 203.0.113.4 5001 typ srflx raddr 0.0.0.0 rport 0\r\n",
        );
        let (rewritten, count) =
            rewrite_mdns_candidates(sdp, Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));

        assert_eq!(count, 1);
        assert!(rewritten.contains("a=candidate:host 1 udp 1 127.0.0.1 5000 typ host"));
        assert!(rewritten.contains("a=candidate:srflx 1 udp 1 203.0.113.4 5001 typ srflx"));
        assert!(!rewritten.contains("browser.local"));
    }
}
