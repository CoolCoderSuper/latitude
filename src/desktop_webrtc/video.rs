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
        BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Profile,
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
struct NativeVideoSettings {
    fps: u16,
    bitrate_kbps: u32,
}

impl NativeVideoSettings {
    const fn new(fps: u16, bitrate_kbps: u32) -> Self {
        Self { fps, bitrate_kbps }
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
    capture: JoinHandle<()>,
    encoder: JoinHandle<()>,
}

impl SharedVideoProducer {
    fn start(settings: NativeVideoSettings, generation: u64) -> Self {
        let (capture_tx, capture_rx) = watch::channel::<Option<Arc<CapturedDesktopEvent>>>(None);
        let (frame_tx, _) = watch::channel::<Option<Arc<EncodedDesktopEvent>>>(None);
        let (stop_tx, stop_rx) = watch::channel(false);
        let capture_stop_rx = stop_rx.clone();
        let capture = tokio::task::spawn_blocking(move || {
            capture_desktop_frames(capture_tx, capture_stop_rx, settings.fps);
        });
        let force_keyframe = Arc::new(AtomicBool::new(false));
        let force_keyframe_for_encoder = Arc::clone(&force_keyframe);
        let frame_tx_for_encoder = frame_tx.clone();
        let runtime = tokio::runtime::Handle::current();
        let encoder = tokio::task::spawn_blocking(move || {
            encode_desktop_frames(
                runtime,
                capture_rx,
                frame_tx_for_encoder,
                stop_rx,
                settings.fps,
                settings.bitrate_kbps,
                force_keyframe_for_encoder,
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
            capture,
            encoder,
        }
    }

    async fn stop(self, settings: NativeVideoSettings) {
        let _ = self.stop_tx.send(true);
        let _ = self.capture.await;
        let _ = self.encoder.await;
        debug!(
            fps = settings.fps,
            bitrate_kbps = settings.bitrate_kbps,
            "shared native desktop video producer stopped"
        );
    }
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

pub(super) async fn request_video_keyframe(fps: u16, bitrate_kbps: u32) {
    native_video_hub()
        .request_keyframe(NativeVideoSettings::new(fps, bitrate_kbps))
        .await;
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
    fps: u16,
    bitrate_kbps: u32,
) -> Result<NativeVideoPipeline> {
    let settings = NativeVideoSettings::new(fps, bitrate_kbps);
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
                        "screens": crate::desktop::detect_desktop_screens(),
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
    frame_tx: watch::Sender<Option<Arc<EncodedDesktopEvent>>>,
    stop_rx: watch::Receiver<bool>,
    fps: u16,
    bitrate_kbps: u32,
    force_keyframe: Arc<AtomicBool>,
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
    let mut conversion_time = Duration::ZERO;
    let mut encode_time = Duration::ZERO;
    let mut has_encoded_frame = false;

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
        if force_keyframe.swap(false, Ordering::AcqRel) && has_encoded_frame {
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
        if !h264.is_empty() {
            frame_tx.send_replace(Some(Arc::new(EncodedDesktopEvent::Frame(
                EncodedDesktopFrame {
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use tokio::sync::watch;

    use super::{
        EncodedDesktopEvent, NativeVideoHubState, NativeVideoSettings, SharedVideoProducer,
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
            capture: tokio::spawn(async {}),
            encoder: tokio::spawn(async {}),
        }
    }

    #[tokio::test]
    async fn shared_producer_stops_only_after_its_last_subscriber_leaves() {
        let settings = NativeVideoSettings::new(30, 4_000);
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
        let settings = NativeVideoSettings::new(30, 4_000);
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
}
