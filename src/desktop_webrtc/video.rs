use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use openh264::{
    OpenH264API,
    encoder::{
        BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Profile,
        RateControlMode, UsageType, VuiConfig,
    },
    formats::YUVSource,
};
use tokio::{
    sync::{RwLock, mpsc, watch},
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

pub(super) struct NativeVideoPipeline {
    stop_tx: watch::Sender<bool>,
    capture: JoinHandle<()>,
    encoder: JoinHandle<()>,
    writer: JoinHandle<()>,
}

impl NativeVideoPipeline {
    pub(super) async fn stop(self) {
        let _ = self.stop_tx.send(true);
        let _ = self.capture.await;
        let _ = self.encoder.await;
        let _ = self.writer.await;
    }
}

pub(super) fn start_video_pipeline(
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
