#[cfg(windows)]
#[path = "gpu.rs"]
mod gpu;

use std::{
    collections::HashMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Result;
use bytes::Bytes;
use openh264::{
    OpenH264API,
    encoder::{
        BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Level, Profile,
        RateControlMode, UsageType, VuiConfig,
    },
    formats::YUVSource,
};
use tokio::{
    sync::{Mutex, RwLock, watch},
    task::JoinHandle,
};
use tracing::{debug, warn};
use webrtc::{
    data_channel::RTCDataChannel, media::Sample,
    track::track_local::track_local_static_sample::TrackLocalStaticSample,
};
use yuvutils_rs::{
    BufferStoreMut, YuvConversionMode, YuvPlanarImageMut, YuvRange, YuvStandardMatrix,
    bgra_to_yuv420,
};

use super::peer::send_control_message;
use crate::desktop::{
    NativeDesktopCapture, NativeDesktopCursor, NativeDesktopFrame, NativeDesktopGeometry,
};

struct EncodedDesktopFrame {
    source_geometry: NativeDesktopGeometry,
    geometry: NativeDesktopGeometry,
    cursor: NativeDesktopCursor,
    captured_at: Instant,
    h264: Bytes,
}

enum CapturedDesktopEvent {
    Frame(NativeDesktopFrame),
    Error(String),
}

enum EncodedDesktopEvent {
    Frame(EncodedDesktopFrame),
    Error(String),
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(super) struct NativeVideoSettings {
    fps: u16,
    bitrate_kbps: u32,
    max_width: u32,
    max_height: u32,
}

impl NativeVideoSettings {
    pub(super) const fn new(fps: u16, bitrate_kbps: u32, max_width: u32, max_height: u32) -> Self {
        Self {
            fps,
            bitrate_kbps,
            max_width,
            max_height,
        }
    }
}

fn h264_level(width: u32, height: u32, fps: u16, bitrate_kbps: u32) -> Level {
    let frame_macroblocks = u64::from(width.div_ceil(16)) * u64::from(height.div_ceil(16));
    let macroblocks_per_second = frame_macroblocks * u64::from(fps);
    for (level, max_frame_macroblocks, max_macroblocks_per_second, max_bitrate_kbps) in [
        (Level::Level_3_1, 3_600, 108_000, 14_000),
        (Level::Level_3_2, 5_120, 216_000, 20_000),
        (Level::Level_4_0, 8_192, 245_760, 20_000),
        (Level::Level_4_1, 8_192, 245_760, 50_000),
        (Level::Level_4_2, 8_704, 522_240, 50_000),
    ] {
        if frame_macroblocks <= max_frame_macroblocks
            && macroblocks_per_second <= max_macroblocks_per_second
            && bitrate_kbps <= max_bitrate_kbps
        {
            return level;
        }
    }

    Level::Level_4_2
}

pub(super) fn h264_profile_level_id(
    width: u32,
    height: u32,
    fps: u16,
    bitrate_kbps: u32,
) -> &'static str {
    match h264_level(width, height, fps, bitrate_kbps) {
        Level::Level_3_1 => "42001f",
        Level::Level_3_2 => "420020",
        Level::Level_4_0 => "420028",
        Level::Level_4_1 => "420029",
        Level::Level_4_2 => "42002a",
        _ => unreachable!("native desktop stream caps require at most H.264 level 4.2"),
    }
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

struct SharedVideoProducer {
    generation: u64,
    subscribers: usize,
    force_keyframe: Arc<AtomicBool>,
    frame_tx: watch::Sender<Option<Arc<EncodedDesktopEvent>>>,
    stop_tx: watch::Sender<bool>,
    worker: JoinHandle<()>,
}

impl SharedVideoProducer {
    fn start(settings: NativeVideoSettings, generation: u64) -> Self {
        let (frame_tx, _) = watch::channel::<Option<Arc<EncodedDesktopEvent>>>(None);
        let (stop_tx, stop_rx) = watch::channel(false);
        let force_keyframe = Arc::new(AtomicBool::new(false));
        let force_keyframe_for_worker = Arc::clone(&force_keyframe);
        let frame_tx_for_worker = frame_tx.clone();
        let runtime = tokio::runtime::Handle::current();
        let worker = tokio::task::spawn_blocking(move || {
            #[cfg(windows)]
            {
                match gpu::run_gpu_video_pipeline(
                    frame_tx_for_worker.clone(),
                    stop_rx.clone(),
                    settings,
                    Arc::clone(&force_keyframe_for_worker),
                ) {
                    Ok(()) => return,
                    Err(_) if *stop_rx.borrow() => return,
                    Err(error) => {
                        warn!(
                            %error,
                            "native desktop GPU pipeline unavailable; using software fallback"
                        );
                    }
                }
            }
            run_software_video_pipeline(
                runtime,
                frame_tx_for_worker,
                stop_rx,
                settings,
                force_keyframe_for_worker,
            );
        });

        debug!(
            fps = settings.fps,
            bitrate_kbps = settings.bitrate_kbps,
            "shared native desktop video producer started"
        );
        Self {
            generation,
            subscribers: 0,
            force_keyframe,
            frame_tx,
            stop_tx,
            worker,
        }
    }

    async fn stop(self, settings: NativeVideoSettings) {
        let _ = self.stop_tx.send(true);
        let _ = self.worker.await;
        debug!(
            fps = settings.fps,
            bitrate_kbps = settings.bitrate_kbps,
            "shared native desktop video producer stopped"
        );
    }
}

fn run_software_video_pipeline(
    runtime: tokio::runtime::Handle,
    frame_tx: watch::Sender<Option<Arc<EncodedDesktopEvent>>>,
    stop_rx: watch::Receiver<bool>,
    settings: NativeVideoSettings,
    force_keyframe: Arc<AtomicBool>,
) {
    debug!(
        fps = settings.fps,
        bitrate_kbps = settings.bitrate_kbps,
        max_width = settings.max_width,
        max_height = settings.max_height,
        "native desktop GDI/OpenH264 fallback started"
    );
    let (capture_tx, capture_rx) = watch::channel::<Option<Arc<CapturedDesktopEvent>>>(None);
    let capture_stop_rx = stop_rx.clone();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            capture_desktop_frames(capture_tx, capture_stop_rx, settings);
        });
        scope.spawn(move || {
            encode_desktop_frames(
                runtime,
                capture_rx,
                frame_tx,
                stop_rx,
                settings,
                force_keyframe,
            );
        });
    });
}

#[derive(Default)]
struct NativeVideoHubState {
    producers: HashMap<NativeVideoSettings, SharedVideoProducer>,
}

impl NativeVideoHubState {
    fn release(
        &mut self,
        settings: NativeVideoSettings,
        generation: u64,
    ) -> Option<SharedVideoProducer> {
        let producer = self.producers.get_mut(&settings)?;
        if producer.generation != generation {
            return None;
        }
        producer.subscribers = producer.subscribers.saturating_sub(1);
        if producer.subscribers == 0 {
            self.producers.remove(&settings)
        } else {
            None
        }
    }
}

struct NativeVideoSubscription {
    generation: u64,
    frame_rx: watch::Receiver<Option<Arc<EncodedDesktopEvent>>>,
}

struct NativeVideoHub {
    next_generation: AtomicU64,
    state: Mutex<NativeVideoHubState>,
}

impl NativeVideoHub {
    fn new() -> Self {
        Self {
            next_generation: AtomicU64::new(1),
            state: Mutex::new(NativeVideoHubState::default()),
        }
    }

    async fn subscribe(&self, settings: NativeVideoSettings) -> Result<NativeVideoSubscription> {
        let mut state = self.state.lock().await;
        let producer = state.producers.entry(settings).or_insert_with(|| {
            let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
            SharedVideoProducer::start(settings, generation)
        });
        producer.subscribers += 1;
        producer.force_keyframe.store(true, Ordering::Release);
        debug!(
            subscribers = producer.subscribers,
            fps = settings.fps,
            bitrate_kbps = settings.bitrate_kbps,
            "native desktop viewer subscribed to shared video producer"
        );

        Ok(NativeVideoSubscription {
            generation: producer.generation,
            frame_rx: producer.frame_tx.subscribe(),
        })
    }

    async fn request_keyframe(&self, settings: NativeVideoSettings) {
        let state = self.state.lock().await;
        if let Some(producer) = state.producers.get(&settings) {
            producer.force_keyframe.store(true, Ordering::Release);
        }
    }

    async fn unsubscribe(&self, settings: NativeVideoSettings, generation: u64) {
        let producer = {
            let mut state = self.state.lock().await;
            let producer = state.release(settings, generation);
            if let Some(active) = state.producers.get(&settings) {
                debug!(
                    subscribers = active.subscribers,
                    fps = settings.fps,
                    bitrate_kbps = settings.bitrate_kbps,
                    "native desktop viewer unsubscribed from shared video producer"
                );
            }
            producer
        };
        if let Some(producer) = producer {
            producer.stop(settings).await;
        }
    }
}

fn native_video_hub() -> &'static NativeVideoHub {
    static HUB: OnceLock<NativeVideoHub> = OnceLock::new();
    HUB.get_or_init(NativeVideoHub::new)
}

pub(super) async fn request_video_keyframe(settings: NativeVideoSettings) {
    native_video_hub().request_keyframe(settings).await;
}

pub(super) struct NativeVideoPipeline {
    settings: NativeVideoSettings,
    generation: Option<u64>,
    writer_stop_tx: Option<watch::Sender<bool>>,
    writer: Option<JoinHandle<()>>,
}

impl NativeVideoPipeline {
    pub(super) async fn stop(mut self) {
        self.stop_writer().await;
        self.unsubscribe().await;
    }

    async fn stop_writer(&mut self) {
        if let Some(stop_tx) = self.writer_stop_tx.take() {
            let _ = stop_tx.send(true);
        }
        if let Some(mut writer) = self.writer.take()
            && tokio::time::timeout(Duration::from_secs(2), &mut writer)
                .await
                .is_err()
        {
            writer.abort();
            let _ = writer.await;
        }
    }

    async fn unsubscribe(&mut self) {
        if let Some(generation) = self.generation.take() {
            native_video_hub()
                .unsubscribe(self.settings, generation)
                .await;
        }
    }
}

impl Drop for NativeVideoPipeline {
    fn drop(&mut self) {
        if let Some(stop_tx) = self.writer_stop_tx.take() {
            let _ = stop_tx.send(true);
        }
        if let Some(writer) = self.writer.take() {
            writer.abort();
        }
        let Some(generation) = self.generation.take() else {
            return;
        };
        let settings = self.settings;
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    native_video_hub().unsubscribe(settings, generation).await;
                });
            }
            Err(error) => {
                warn!(%error, "native desktop video subscription could not be released");
            }
        }
    }
}

pub(super) async fn start_video_pipeline(
    track: Arc<TrackLocalStaticSample>,
    control_channel: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    settings: NativeVideoSettings,
) -> Result<NativeVideoPipeline> {
    let NativeVideoSubscription {
        generation,
        frame_rx,
        ..
    } = native_video_hub().subscribe(settings).await?;
    let (writer_stop_tx, writer_stop_rx) = watch::channel(false);
    let writer = tokio::spawn(async move {
        write_video_frames(track, control_channel, frame_rx, writer_stop_rx, settings).await;
    });

    Ok(NativeVideoPipeline {
        settings,
        generation: Some(generation),
        writer_stop_tx: Some(writer_stop_tx),
        writer: Some(writer),
    })
}

async fn write_video_frames(
    track: Arc<TrackLocalStaticSample>,
    control_channel: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    mut frame_rx: watch::Receiver<Option<Arc<EncodedDesktopEvent>>>,
    mut stop_rx: watch::Receiver<bool>,
    settings: NativeVideoSettings,
) {
    let nominal_duration = Duration::from_secs_f64(1.0 / f64::from(settings.fps.max(1)));
    let mut last_capture = None;
    let mut last_geometry = None;
    let mut last_cursor = None;
    let mut stats_started = Instant::now();
    let mut sent_frames = 0_u64;
    let mut sent_bytes = 0_u64;
    let mut pending = frame_rx.borrow_and_update().clone();

    loop {
        if let Some(event) = pending.take() {
            let frame = match event.as_ref() {
                EncodedDesktopEvent::Frame(frame) => frame,
                EncodedDesktopEvent::Error(error) => {
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
                        "screens": crate::desktop::scale_native_desktop_screens(
                            crate::desktop::detect_desktop_screens(),
                            frame.source_geometry,
                            frame.geometry,
                        ),
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
            let duration = last_capture
                .replace(frame.captured_at)
                .map(|previous| frame.captured_at.saturating_duration_since(previous))
                .filter(|duration| !duration.is_zero())
                .unwrap_or(nominal_duration);
            if let Err(error) = track
                .write_sample(&Sample {
                    data: frame.h264.clone(),
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

        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow_and_update() {
                    break;
                }
            }
            changed = frame_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                pending = frame_rx.borrow_and_update().clone();
            }
        }
    }
}

fn capture_desktop_frames(
    frame_tx: watch::Sender<Option<Arc<CapturedDesktopEvent>>>,
    stop_rx: watch::Receiver<bool>,
    settings: NativeVideoSettings,
) {
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(settings.fps.max(1)));
    let mut stats_started = Instant::now();
    let mut captured_frames = 0_u64;
    let mut capture_time = Duration::ZERO;
    let mut capture = match NativeDesktopCapture::new(settings.max_width, settings.max_height) {
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
    let remaining = deadline.saturating_duration_since(Instant::now());
    if !remaining.is_zero() {
        std::thread::sleep(remaining);
    }
}

fn encode_desktop_frames(
    runtime: tokio::runtime::Handle,
    mut capture_rx: watch::Receiver<Option<Arc<CapturedDesktopEvent>>>,
    frame_tx: watch::Sender<Option<Arc<EncodedDesktopEvent>>>,
    stop_rx: watch::Receiver<bool>,
    settings: NativeVideoSettings,
    force_keyframe: Arc<AtomicBool>,
) {
    let fps = settings.fps.max(1);
    let encoder_config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(
            settings.bitrate_kbps.saturating_mul(1_000),
        ))
        .max_frame_rate(FrameRate::from_hz(fps as f32))
        .rate_control_mode(RateControlMode::Bitrate)
        .usage_type(UsageType::ScreenContentRealTime)
        .profile(Profile::Baseline)
        .level(h264_level(
            settings.max_width,
            settings.max_height,
            fps,
            settings.bitrate_kbps,
        ))
        .complexity(Complexity::Low)
        .adaptive_quantization(false)
        .background_detection(false)
        .intra_frame_period(IntraFramePeriod::from_num_frames(u32::from(fps) * 2))
        .vui(VuiConfig::srgb())
        .skip_frames(true);
    let mut encoder = match Encoder::with_api_config(OpenH264API::from_source(), encoder_config) {
        Ok(encoder) => encoder,
        Err(error) => {
            frame_tx.send_replace(Some(Arc::new(EncodedDesktopEvent::Error(format!(
                "H.264 encoder could not be initialized: {error}"
            )))));
            return;
        }
    };
    let mut yuv = None;
    let mut stats_started = Instant::now();
    let mut attempted_frames = 0_u64;
    let mut encoded_frames = 0_u64;
    let mut unchanged_frames = 0_u64;
    let mut conversion_time = Duration::ZERO;
    let mut encode_time = Duration::ZERO;
    let mut has_encoded_frame = false;
    let mut previous_encoded_event: Option<Arc<CapturedDesktopEvent>> = None;

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
                frame_tx.send_replace(Some(Arc::new(EncodedDesktopEvent::Error(error.clone()))));
                break;
            }
        };
        let force_requested = force_keyframe.swap(false, Ordering::AcqRel);
        let unchanged = previous_encoded_event
            .as_deref()
            .and_then(|event| match event {
                CapturedDesktopEvent::Frame(frame) => Some(frame),
                CapturedDesktopEvent::Error(_) => None,
            })
            .is_some_and(|previous| {
                previous.source_geometry == frame.source_geometry
                    && previous.geometry == frame.geometry
                    && previous.cursor == frame.cursor
                    && *previous.bgra == *frame.bgra
            });
        if unchanged && !force_requested {
            unchanged_frames += 1;
            continue;
        }
        let dimensions = (
            frame.geometry.width as usize,
            frame.geometry.height as usize,
        );
        let buffer = yuv.get_or_insert_with(|| DesktopYuvBuffer::new(dimensions.0, dimensions.1));
        buffer.resize(dimensions.0, dimensions.1);
        let conversion_started = Instant::now();
        if let Err(error) = buffer.read_bgra(&frame.bgra) {
            frame_tx.send_replace(Some(Arc::new(EncodedDesktopEvent::Error(format!(
                "desktop frame could not be converted to YUV: {error}"
            )))));
            break;
        }
        conversion_time += conversion_started.elapsed();
        if force_requested && has_encoded_frame {
            encoder.force_intra_frame();
        }
        let encode_started = Instant::now();
        let bitstream = match encoder.encode(buffer) {
            Ok(bitstream) => bitstream,
            Err(error) => {
                frame_tx.send_replace(Some(Arc::new(EncodedDesktopEvent::Error(format!(
                    "desktop frame could not be encoded as H.264: {error}"
                )))));
                break;
            }
        };
        encode_time += encode_started.elapsed();
        let h264 = Bytes::from(bitstream.to_vec());
        attempted_frames += 1;
        encoded_frames += u64::from(!h264.is_empty());
        has_encoded_frame |= !h264.is_empty();
        if h264.is_empty() && force_requested {
            force_keyframe.store(true, Ordering::Release);
        }
        if !h264.is_empty() {
            previous_encoded_event = Some(Arc::clone(&event));
            frame_tx.send_replace(Some(Arc::new(EncodedDesktopEvent::Frame(
                EncodedDesktopFrame {
                    source_geometry: frame.source_geometry,
                    geometry: frame.geometry,
                    cursor: frame.cursor,
                    captured_at: frame.captured_at,
                    h264,
                },
            ))));
        }

        let stats_elapsed = stats_started.elapsed();
        if stats_elapsed >= Duration::from_secs(5) {
            debug!(
                attempted_frames_per_second = attempted_frames as f64 / stats_elapsed.as_secs_f64(),
                encoded_frames_per_second = encoded_frames as f64 / stats_elapsed.as_secs_f64(),
                average_conversion_ms =
                    conversion_time.as_secs_f64() * 1_000.0 / attempted_frames as f64,
                average_encode_ms = encode_time.as_secs_f64() * 1_000.0 / attempted_frames as f64,
                unchanged_frames_per_second = unchanged_frames as f64 / stats_elapsed.as_secs_f64(),
                "native WebRTC desktop producer rate"
            );
            stats_started = Instant::now();
            attempted_frames = 0;
            encoded_frames = 0;
            unchanged_frames = 0;
            conversion_time = Duration::ZERO;
            encode_time = Duration::ZERO;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use tokio::sync::watch;

    use super::{
        EncodedDesktopEvent, NativeVideoHubState, NativeVideoSettings, SharedVideoProducer,
        h264_profile_level_id,
    };

    fn fake_producer(generation: u64, subscribers: usize) -> SharedVideoProducer {
        let (frame_tx, _) = watch::channel::<Option<Arc<EncodedDesktopEvent>>>(None);
        let (stop_tx, _) = watch::channel(false);
        SharedVideoProducer {
            generation,
            subscribers,
            force_keyframe: Arc::new(AtomicBool::new(false)),
            frame_tx,
            stop_tx,
            worker: tokio::spawn(async {}),
        }
    }

    #[tokio::test]
    async fn shared_producer_stops_only_after_its_last_subscriber_leaves() {
        let settings = NativeVideoSettings::new(30, 4_000, 1_920, 1_080);
        let mut state = NativeVideoHubState::default();
        state.producers.insert(settings, fake_producer(7, 2));

        assert!(state.release(settings, 7).is_none());
        assert_eq!(state.producers[&settings].subscribers, 1);

        let producer = state.release(settings, 7).unwrap();
        assert!(state.producers.is_empty());
        producer.stop(settings).await;
    }

    #[tokio::test]
    async fn stale_subscription_cannot_release_a_replacement_producer() {
        let settings = NativeVideoSettings::new(30, 4_000, 1_920, 1_080);
        let mut state = NativeVideoHubState::default();
        state.producers.insert(settings, fake_producer(9, 1));

        assert!(state.release(settings, 8).is_none());
        assert_eq!(state.producers[&settings].subscribers, 1);
        assert!(
            !state.producers[&settings]
                .force_keyframe
                .load(Ordering::Acquire)
        );

        let producer = state.release(settings, 9).unwrap();
        producer.stop(settings).await;
    }

    #[test]
    fn advertises_the_level_required_by_the_stream_cap() {
        assert_eq!(h264_profile_level_id(1_280, 720, 30, 4_000), "42001f");
        assert_eq!(h264_profile_level_id(1_920, 1_080, 30, 4_000), "420028");
        assert_eq!(h264_profile_level_id(1_920, 1_080, 60, 4_000), "42002a");
        assert_eq!(h264_profile_level_id(1_920, 1_080, 30, 25_000), "420029");
    }
}
