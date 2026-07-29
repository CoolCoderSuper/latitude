use std::{
    ffi::c_void,
    mem::{ManuallyDrop, size_of},
    ptr::null_mut,
    slice,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use bytes::Bytes;
use tokio::sync::watch;
use tracing::debug;
use windows::{
    Win32::{
        Foundation::{HMODULE, RECT, VARIANT_TRUE},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0},
            Direct3D11::{
                D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_FLAG,
                D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION, D3D11_TEX2D_VPIV,
                D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC,
                D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT,
                D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
                D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
                D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
                D3D11_VIDEO_USAGE_PLAYBACK_NORMAL, D3D11_VPIV_DIMENSION_TEXTURE2D,
                D3D11_VPOV_DIMENSION_TEXTURE2D, D3D11CreateDevice, ID3D11Device,
                ID3D11DeviceContext, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoContext1,
                ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorInputView,
                ID3D11VideoProcessorOutputView,
            },
            Dxgi::{
                Common::{
                    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
                    DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709, DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_FORMAT_NV12, DXGI_MODE_ROTATION_IDENTITY, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_NOT_FOUND,
                DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTDUPL_MOVE_RECT,
                IDXGIAdapter1, IDXGIFactory1, IDXGIOutput, IDXGIOutput1, IDXGIOutput5,
                IDXGIOutputDuplication, IDXGIResource,
            },
        },
        Media::MediaFoundation::{
            CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
            CODECAPI_AVEncCommonRealTime, CODECAPI_AVEncMPVGOPSize,
            CODECAPI_AVEncVideoForceKeyFrame, CODECAPI_AVLowLatencyMode, ICodecAPI, IMFActivate,
            IMFAttributes, IMFDXGIDeviceManager, IMFMediaBuffer, IMFMediaEventGenerator,
            IMFMediaType, IMFSample, IMFTransform, METransformHaveOutput, METransformNeedInput,
            MF_E_NO_EVENTS_AVAILABLE, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_EVENT_FLAG_NO_WAIT,
            MF_LOW_LATENCY, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
            MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_MPEG2_LEVEL,
            MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_SA_D3D11_AWARE,
            MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION, MFCreateAttributes, MFCreateDXGIDeviceManager,
            MFCreateDXGISurfaceBuffer, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample,
            MFMediaType_Video, MFSTARTUP_NOSOCKET, MFShutdown, MFStartup,
            MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_ADAPTER_LUID, MFT_ENUM_FLAG,
            MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_MESSAGE_COMMAND_FLUSH,
            MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_END_STREAMING,
            MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_MESSAGE_SET_D3D_MANAGER,
            MFT_OUTPUT_DATA_BUFFER, MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES,
            MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFT_REGISTER_TYPE_INFO, MFTEnum2,
            MFVideoFormat_H264, MFVideoFormat_NV12, MFVideoInterlace_Progressive,
            eAVEncCommonRateControlMode_CBR, eAVEncH264VProfile_Base,
        },
        System::{
            Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
            Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_UI4},
        },
    },
    core::Interface,
};

use super::{EncodedDesktopEvent, EncodedDesktopFrame, NativeVideoSettings};
use crate::desktop::{NativeDesktopGeometry, fit_native_desktop_geometry, native_cursor_style};

const ENCODER_EVENT_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_D3D11_TEXTURE_DIMENSION: u32 = 16_384;

pub(super) fn run_gpu_video_pipeline(
    frame_tx: watch::Sender<Option<Arc<EncodedDesktopEvent>>>,
    stop_rx: watch::Receiver<bool>,
    settings: NativeVideoSettings,
    force_keyframe: Arc<AtomicBool>,
) -> Result<()> {
    let _runtime = WindowsMediaRuntime::new()?;
    let mut pipeline = GpuVideoPipeline::new(settings)?;
    debug!(
        adapter = %pipeline.adapter_name,
        outputs = pipeline.outputs.len(),
        width = pipeline.geometry.width,
        height = pipeline.geometry.height,
        "native desktop GPU capture and hardware encoder started"
    );

    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(settings.fps.max(1)));
    let mut next_frame = Instant::now();
    let mut stats_started = Instant::now();
    let mut encoded_frames = 0_u64;
    let mut encoded_bytes = 0_u64;
    let mut last_cursor = None;

    while !*stop_rx.borrow() && !frame_tx.is_closed() {
        let mut changed = false;
        for output in &mut pipeline.outputs {
            changed |= output.update(&pipeline.context, &pipeline.composite)?;
        }
        let initialized = pipeline.outputs.iter().all(|output| output.initialized);
        let force = force_keyframe.swap(false, Ordering::AcqRel);
        let cursor = native_cursor_style();
        let cursor_changed = last_cursor != Some(cursor);
        if initialized && (changed || force || cursor_changed) {
            let captured_at = Instant::now();
            pipeline.processor.process()?;
            let encoded =
                pipeline
                    .encoder
                    .encode(&pipeline.processor.nv12, captured_at, force, || {
                        *stop_rx.borrow() || frame_tx.is_closed()
                    })?;
            if let Some(h264) = encoded {
                encoded_frames += 1;
                encoded_bytes += h264.len() as u64;
                frame_tx.send_replace(Some(Arc::new(EncodedDesktopEvent::Frame(
                    EncodedDesktopFrame {
                        source_geometry: pipeline.source_geometry,
                        geometry: pipeline.geometry,
                        cursor,
                        captured_at,
                        h264,
                    },
                ))));
                last_cursor = Some(cursor);
            } else if force {
                force_keyframe.store(true, Ordering::Release);
            }
        }

        let elapsed = stats_started.elapsed();
        if elapsed >= Duration::from_secs(5) {
            debug!(
                frames_per_second = encoded_frames as f64 / elapsed.as_secs_f64(),
                payload_kbps = encoded_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1_000.0,
                "native desktop GPU producer rate"
            );
            stats_started = Instant::now();
            encoded_frames = 0;
            encoded_bytes = 0;
        }

        next_frame += frame_interval;
        let remaining = next_frame.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            std::thread::sleep(remaining);
        } else {
            next_frame = Instant::now();
        }
    }

    Ok(())
}

struct WindowsMediaRuntime {
    com_initialized: bool,
    media_foundation_started: bool,
}

impl WindowsMediaRuntime {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .context("COM could not be initialized for native GPU video")?;
        }
        let mut runtime = Self {
            com_initialized: true,
            media_foundation_started: false,
        };
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET)
                .context("Media Foundation could not be started")?;
        }
        runtime.media_foundation_started = true;
        Ok(runtime)
    }
}

impl Drop for WindowsMediaRuntime {
    fn drop(&mut self) {
        unsafe {
            if self.media_foundation_started {
                let _ = MFShutdown();
            }
            if self.com_initialized {
                CoUninitialize();
            }
        }
    }
}

struct GpuVideoPipeline {
    adapter_name: String,
    context: ID3D11DeviceContext,
    source_geometry: NativeDesktopGeometry,
    geometry: NativeDesktopGeometry,
    outputs: Vec<DuplicatedOutput>,
    composite: ID3D11Texture2D,
    processor: GpuFrameProcessor,
    encoder: HardwareH264Encoder,
}

type DesktopOutputDescription = (
    IDXGIOutput,
    windows::Win32::Graphics::Dxgi::DXGI_OUTPUT_DESC,
);
type DesktopAdapterSelection = (IDXGIAdapter1, String, Vec<DesktopOutputDescription>);

impl GpuVideoPipeline {
    fn new(settings: NativeVideoSettings) -> Result<Self> {
        let (adapter, adapter_name, output_descriptions) = select_desktop_adapter()?;
        let source_geometry = geometry_for_outputs(&output_descriptions)?;
        if source_geometry.width > MAX_D3D11_TEXTURE_DIMENSION
            || source_geometry.height > MAX_D3D11_TEXTURE_DIMENSION
        {
            bail!(
                "virtual desktop {}x{} exceeds the D3D11 texture limit",
                source_geometry.width,
                source_geometry.height
            );
        }
        let geometry =
            fit_native_desktop_geometry(source_geometry, settings.max_width, settings.max_height);
        let (device, context) = create_d3d11_device(&adapter)?;
        let composite = create_texture(
            &device,
            source_geometry.width,
            source_geometry.height,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            D3D11_BIND_SHADER_RESOURCE.0 as u32 | D3D11_BIND_RENDER_TARGET.0 as u32,
        )?;
        clear_texture(&device, &context, &composite)?;
        let outputs = output_descriptions
            .into_iter()
            .map(|(output, desc)| {
                DuplicatedOutput::new(
                    &device,
                    output,
                    desc.DesktopCoordinates,
                    source_geometry.origin_x,
                    source_geometry.origin_y,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let processor = GpuFrameProcessor::new(
            &device,
            &context,
            &composite,
            source_geometry,
            geometry,
            settings.fps,
        )?;
        let adapter_desc =
            unsafe { adapter.GetDesc1() }.context("DXGI adapter description failed")?;
        let encoder = HardwareH264Encoder::new(
            &device,
            adapter_desc.AdapterLuid.LowPart,
            adapter_desc.AdapterLuid.HighPart,
            geometry,
            settings,
        )?;

        Ok(Self {
            adapter_name,
            context,
            source_geometry,
            geometry,
            outputs,
            composite,
            processor,
            encoder,
        })
    }
}

fn select_desktop_adapter() -> Result<DesktopAdapterSelection> {
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.context("DXGI factory could not be created")?;
    let mut adapters = Vec::new();
    let mut adapter_index = 0;
    loop {
        let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error).context("DXGI adapter enumeration failed"),
        };
        adapter_index += 1;
        let mut outputs = Vec::new();
        let mut output_index = 0;
        loop {
            let output = match unsafe { adapter.EnumOutputs(output_index) } {
                Ok(output) => output,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => {
                    return Err(error).context("DXGI output enumeration failed");
                }
            };
            output_index += 1;
            let desc = unsafe { output.GetDesc() }.context("DXGI output description failed")?;
            if !desc.AttachedToDesktop.as_bool() {
                continue;
            }
            if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY {
                bail!("rotated displays require the software capture fallback");
            }
            outputs.push((output, desc));
        }
        if !outputs.is_empty() {
            adapters.push((adapter, outputs));
        }
    }

    if adapters.is_empty() {
        bail!("DXGI did not report an attached desktop output");
    }
    if adapters.len() != 1 {
        bail!("desktops spanning multiple graphics adapters use the software fallback");
    }
    let (adapter, outputs) = adapters.pop().unwrap();
    let desc = unsafe { adapter.GetDesc1() }.context("DXGI adapter description failed")?;
    let adapter_name = String::from_utf16_lossy(
        &desc.Description[..desc
            .Description
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(128)],
    );
    Ok((adapter, adapter_name, outputs))
}

fn geometry_for_outputs(
    outputs: &[(
        IDXGIOutput,
        windows::Win32::Graphics::Dxgi::DXGI_OUTPUT_DESC,
    )],
) -> Result<NativeDesktopGeometry> {
    let left = outputs
        .iter()
        .map(|(_, desc)| desc.DesktopCoordinates.left)
        .min()
        .ok_or_else(|| anyhow!("DXGI did not report an attached output"))?;
    let top = outputs
        .iter()
        .map(|(_, desc)| desc.DesktopCoordinates.top)
        .min()
        .ok_or_else(|| anyhow!("DXGI did not report an attached output"))?;
    let right = outputs
        .iter()
        .map(|(_, desc)| desc.DesktopCoordinates.right)
        .max()
        .ok_or_else(|| anyhow!("DXGI did not report an attached output"))?;
    let bottom = outputs
        .iter()
        .map(|(_, desc)| desc.DesktopCoordinates.bottom)
        .max()
        .ok_or_else(|| anyhow!("DXGI did not report an attached output"))?;
    let width =
        u32::try_from(right - left).context("DXGI reported an invalid virtual desktop width")?;
    let height =
        u32::try_from(bottom - top).context("DXGI reported an invalid virtual desktop height")?;
    if width < 2 || height < 2 {
        bail!("DXGI reported an empty virtual desktop");
    }
    Ok(NativeDesktopGeometry {
        origin_x: left,
        origin_y: top,
        width,
        height,
    })
}

fn create_d3d11_device(adapter: &IDXGIAdapter1) -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    let flags = D3D11_CREATE_DEVICE_FLAG(
        D3D11_CREATE_DEVICE_BGRA_SUPPORT.0 | D3D11_CREATE_DEVICE_VIDEO_SUPPORT.0,
    );
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            flags,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .context("D3D11 device could not be created")?;
    }
    Ok((
        device.ok_or_else(|| anyhow!("D3D11 returned no device"))?,
        context.ok_or_else(|| anyhow!("D3D11 returned no immediate context"))?,
    ))
}

fn create_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    bind_flags: u32,
) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: bind_flags,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut texture))
            .context("D3D11 video texture could not be created")?;
    }
    texture.ok_or_else(|| anyhow!("D3D11 returned no video texture"))
}

fn clear_texture(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
) -> Result<()> {
    let mut view = None;
    unsafe {
        device
            .CreateRenderTargetView(texture, None, Some(&mut view))
            .context("D3D11 desktop render target could not be created")?;
    }
    let view = view.ok_or_else(|| anyhow!("D3D11 returned no desktop render target"))?;
    unsafe {
        context.ClearRenderTargetView(&view, &[0.0, 0.0, 0.0, 1.0]);
    }
    Ok(())
}

struct DuplicatedOutput {
    duplication: IDXGIOutputDuplication,
    destination_x: u32,
    destination_y: u32,
    width: u32,
    height: u32,
    initialized: bool,
}

impl DuplicatedOutput {
    fn new(
        device: &ID3D11Device,
        output: IDXGIOutput,
        coordinates: RECT,
        virtual_left: i32,
        virtual_top: i32,
    ) -> Result<Self> {
        let duplication = output
            .cast::<IDXGIOutput5>()
            .and_then(|output5| unsafe {
                output5.DuplicateOutput1(device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM])
            })
            .or_else(|_| {
                let output1: windows::core::Result<IDXGIOutput1> = output.cast();
                output1.and_then(|output1| unsafe { output1.DuplicateOutput(device) })
            })
            .context("DXGI desktop duplication could not be created")?;
        Ok(Self {
            duplication,
            destination_x: u32::try_from(coordinates.left - virtual_left)
                .context("DXGI output has an invalid horizontal offset")?,
            destination_y: u32::try_from(coordinates.top - virtual_top)
                .context("DXGI output has an invalid vertical offset")?,
            width: u32::try_from(coordinates.right - coordinates.left)
                .context("DXGI output has an invalid width")?,
            height: u32::try_from(coordinates.bottom - coordinates.top)
                .context("DXGI output has an invalid height")?,
            initialized: false,
        })
    }

    fn update(
        &mut self,
        context: &ID3D11DeviceContext,
        composite: &ID3D11Texture2D,
    ) -> Result<bool> {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        match unsafe {
            self.duplication
                .AcquireNextFrame(0, &mut info, &mut resource)
        } {
            Ok(()) => {}
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
            Err(error) if error.code() == DXGI_ERROR_ACCESS_LOST => {
                return Err(error).context("DXGI desktop duplication access was lost");
            }
            Err(error) => {
                return Err(error).context("DXGI desktop frame acquisition failed");
            }
        }

        let update_result: Result<bool> = (|| {
            let resource = resource.ok_or_else(|| anyhow!("DXGI returned no desktop surface"))?;
            let texture: ID3D11Texture2D = resource.cast()?;
            let mut copy_full = !self.initialized || info.RectsCoalesced.as_bool();
            let move_rects = self.move_rects(info.TotalMetadataBufferSize)?;
            copy_full |= !move_rects.is_empty();
            let dirty_rects = if copy_full {
                Vec::new()
            } else {
                self.dirty_rects(info.TotalMetadataBufferSize)?
            };
            let changed = if copy_full {
                self.copy_full(context, composite, &texture);
                true
            } else if dirty_rects.is_empty() {
                false
            } else {
                for rect in dirty_rects {
                    self.copy_rect(context, composite, &texture, rect);
                }
                true
            };
            drop(texture);
            drop(resource);
            Ok(changed)
        })();
        let release_result = unsafe { self.duplication.ReleaseFrame() }
            .context("DXGI desktop frame could not be released");
        let changed = update_result?;
        release_result?;
        if changed {
            self.initialized = true;
        }
        Ok(changed)
    }

    fn move_rects(&self, metadata_bytes: u32) -> Result<Vec<DXGI_OUTDUPL_MOVE_RECT>> {
        if metadata_bytes == 0 {
            return Ok(Vec::new());
        }
        let capacity = metadata_bytes as usize / size_of::<DXGI_OUTDUPL_MOVE_RECT>() + 1;
        let mut rects = Vec::<DXGI_OUTDUPL_MOVE_RECT>::with_capacity(capacity);
        let mut required = 0;
        unsafe {
            self.duplication
                .GetFrameMoveRects(
                    (capacity * size_of::<DXGI_OUTDUPL_MOVE_RECT>()) as u32,
                    rects.as_mut_ptr(),
                    &mut required,
                )
                .context("DXGI move rectangles could not be read")?;
            rects.set_len(required as usize / size_of::<DXGI_OUTDUPL_MOVE_RECT>());
        }
        Ok(rects)
    }

    fn dirty_rects(&self, metadata_bytes: u32) -> Result<Vec<RECT>> {
        if metadata_bytes == 0 {
            return Ok(Vec::new());
        }
        let capacity = metadata_bytes as usize / size_of::<RECT>() + 1;
        let mut rects = Vec::<RECT>::with_capacity(capacity);
        let mut required = 0;
        unsafe {
            self.duplication
                .GetFrameDirtyRects(
                    (capacity * size_of::<RECT>()) as u32,
                    rects.as_mut_ptr(),
                    &mut required,
                )
                .context("DXGI dirty rectangles could not be read")?;
            rects.set_len(required as usize / size_of::<RECT>());
        }
        Ok(rects)
    }

    fn copy_full(
        &self,
        context: &ID3D11DeviceContext,
        composite: &ID3D11Texture2D,
        source: &ID3D11Texture2D,
    ) {
        let source_box = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: self.width,
            bottom: self.height,
            back: 1,
        };
        unsafe {
            context.CopySubresourceRegion(
                composite,
                0,
                self.destination_x,
                self.destination_y,
                0,
                source,
                0,
                Some(&source_box),
            );
        }
    }

    fn copy_rect(
        &self,
        context: &ID3D11DeviceContext,
        composite: &ID3D11Texture2D,
        source: &ID3D11Texture2D,
        rect: RECT,
    ) {
        let width = self.width as i32;
        let height = self.height as i32;
        let left = rect.left.clamp(0, width) as u32;
        let top = rect.top.clamp(0, height) as u32;
        let right = rect.right.clamp(0, width) as u32;
        let bottom = rect.bottom.clamp(0, height) as u32;
        if right <= left || bottom <= top {
            return;
        }
        let source_box = D3D11_BOX {
            left,
            top,
            front: 0,
            right,
            bottom,
            back: 1,
        };
        unsafe {
            context.CopySubresourceRegion(
                composite,
                0,
                self.destination_x + left,
                self.destination_y + top,
                0,
                source,
                0,
                Some(&source_box),
            );
        }
    }
}

struct GpuFrameProcessor {
    video_context: ID3D11VideoContext,
    video_context1: Option<ID3D11VideoContext1>,
    processor: ID3D11VideoProcessor,
    input_view: ID3D11VideoProcessorInputView,
    output_view: ID3D11VideoProcessorOutputView,
    nv12: ID3D11Texture2D,
}

impl GpuFrameProcessor {
    fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        composite: &ID3D11Texture2D,
        source: NativeDesktopGeometry,
        output: NativeDesktopGeometry,
        fps: u16,
    ) -> Result<Self> {
        let video_device: ID3D11VideoDevice = device.cast()?;
        let video_context: ID3D11VideoContext = context.cast()?;
        let video_context1 = video_context.cast::<ID3D11VideoContext1>().ok();
        let content_desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: u32::from(fps.max(1)),
                Denominator: 1,
            },
            InputWidth: source.width,
            InputHeight: source.height,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: u32::from(fps.max(1)),
                Denominator: 1,
            },
            OutputWidth: output.width,
            OutputHeight: output.height,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };
        let enumerator = unsafe { video_device.CreateVideoProcessorEnumerator(&content_desc) }
            .context("D3D11 video processor enumerator could not be created")?;
        let bgra_support =
            unsafe { enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM) }
                .context("D3D11 BGRA processor support could not be queried")?;
        let nv12_support = unsafe { enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_NV12) }
            .context("D3D11 NV12 processor support could not be queried")?;
        if bgra_support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32 == 0
            || nv12_support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 == 0
        {
            bail!("D3D11 video processor does not support BGRA to NV12 conversion");
        }
        let processor = unsafe { video_device.CreateVideoProcessor(&enumerator, 0) }
            .context("D3D11 video processor could not be created")?;
        let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut input_view = None;
        unsafe {
            video_device
                .CreateVideoProcessorInputView(
                    composite,
                    &enumerator,
                    &input_desc,
                    Some(&mut input_view),
                )
                .context("D3D11 video processor input view could not be created")?;
        }
        let nv12 = create_texture(
            device,
            output.width,
            output.height,
            DXGI_FORMAT_NV12,
            D3D11_BIND_RENDER_TARGET.0 as u32 | D3D11_BIND_SHADER_RESOURCE.0 as u32,
        )?;
        let output_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let mut output_view = None;
        unsafe {
            video_device
                .CreateVideoProcessorOutputView(
                    &nv12,
                    &enumerator,
                    &output_desc,
                    Some(&mut output_view),
                )
                .context("D3D11 video processor output view could not be created")?;
        }
        let source_rect = RECT {
            left: 0,
            top: 0,
            right: source.width as i32,
            bottom: source.height as i32,
        };
        let output_rect = RECT {
            left: 0,
            top: 0,
            right: output.width as i32,
            bottom: output.height as i32,
        };
        unsafe {
            video_context.VideoProcessorSetStreamFrameFormat(
                &processor,
                0,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            );
            video_context.VideoProcessorSetStreamSourceRect(
                &processor,
                0,
                true,
                Some(&source_rect),
            );
            video_context.VideoProcessorSetStreamDestRect(&processor, 0, true, Some(&output_rect));
            video_context.VideoProcessorSetOutputTargetRect(&processor, true, Some(&output_rect));
            video_context.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
            if let Some(context1) = &video_context1 {
                context1.VideoProcessorSetStreamColorSpace1(
                    &processor,
                    0,
                    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
                );
                context1.VideoProcessorSetOutputColorSpace1(
                    &processor,
                    DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
                );
            }
        }
        Ok(Self {
            video_context,
            video_context1,
            processor,
            input_view: input_view
                .ok_or_else(|| anyhow!("D3D11 returned no processor input view"))?,
            output_view: output_view
                .ok_or_else(|| anyhow!("D3D11 returned no processor output view"))?,
            nv12,
        })
    }

    fn process(&self) -> Result<()> {
        let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            pInputSurface: ManuallyDrop::new(Some(self.input_view.clone())),
            ..Default::default()
        };
        let result = unsafe {
            self.video_context.VideoProcessorBlt(
                &self.processor,
                &self.output_view,
                0,
                slice::from_ref(&stream),
            )
        }
        .context("D3D11 desktop frame conversion failed");
        unsafe {
            ManuallyDrop::drop(&mut stream.pInputSurface);
        }
        result
    }
}

impl Drop for GpuFrameProcessor {
    fn drop(&mut self) {
        let _ = self.video_context1.take();
    }
}

struct EncoderEvents {
    needs_input: usize,
    has_output: usize,
}

struct HardwareH264Encoder {
    transform: IMFTransform,
    _device_manager: IMFDXGIDeviceManager,
    event_generator: IMFMediaEventGenerator,
    codec_api: Option<ICodecAPI>,
    output_stream_provides_samples: bool,
    output_buffer_size: u32,
    frame_duration_hns: i64,
    next_sample_time_hns: i64,
    events: EncoderEvents,
    parameter_sets: Vec<u8>,
}

impl HardwareH264Encoder {
    fn new(
        device: &ID3D11Device,
        adapter_luid_low: u32,
        adapter_luid_high: i32,
        geometry: NativeDesktopGeometry,
        settings: NativeVideoSettings,
    ) -> Result<Self> {
        let transform = enumerate_hardware_encoder(adapter_luid_low, adapter_luid_high)?;
        let attributes = unsafe { transform.GetAttributes() }
            .context("hardware H.264 encoder attributes were unavailable")?;
        unsafe {
            attributes
                .SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)
                .context("hardware H.264 encoder could not be unlocked")?;
            let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);
        }
        if unsafe { attributes.GetUINT32(&MF_SA_D3D11_AWARE) }.unwrap_or(0) == 0 {
            bail!("hardware H.264 encoder is not D3D11-aware");
        }

        let mut reset_token = 0;
        let mut manager: Option<IMFDXGIDeviceManager> = None;
        unsafe {
            MFCreateDXGIDeviceManager(&mut reset_token, &mut manager)
                .context("Media Foundation DXGI device manager could not be created")?;
        }
        let manager =
            manager.ok_or_else(|| anyhow!("Media Foundation returned no DXGI device manager"))?;
        unsafe {
            manager
                .ResetDevice(device, reset_token)
                .context("Media Foundation could not attach the D3D11 device")?;
            transform
                .ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize)
                .context("hardware H.264 encoder rejected the D3D11 device")?;
        }

        let output_type = create_video_type(MFVideoFormat_H264, geometry, settings, true)?;
        let input_type = create_video_type(MFVideoFormat_NV12, geometry, settings, false)?;
        unsafe {
            transform
                .SetOutputType(0, &output_type, 0)
                .context("hardware H.264 output type was rejected")?;
            transform
                .SetInputType(0, &input_type, 0)
                .context("hardware H.264 NV12 input type was rejected")?;
        }

        let codec_api = transform.cast::<ICodecAPI>().ok();
        if let Some(codec_api) = &codec_api {
            let _ = set_codec_u32(
                codec_api,
                &CODECAPI_AVEncCommonRateControlMode,
                eAVEncCommonRateControlMode_CBR.0 as u32,
            );
            let _ = set_codec_u32(
                codec_api,
                &CODECAPI_AVEncCommonMeanBitRate,
                settings.bitrate_kbps.saturating_mul(1_000),
            );
            let _ = set_codec_bool(codec_api, &CODECAPI_AVEncCommonRealTime, true);
            let _ = set_codec_bool(codec_api, &CODECAPI_AVLowLatencyMode, true);
            let _ = set_codec_u32(
                codec_api,
                &CODECAPI_AVEncMPVGOPSize,
                u32::from(settings.fps.max(1)) * 2,
            );
        }

        let stream_info = unsafe { transform.GetOutputStreamInfo(0) }
            .context("hardware H.264 output stream information was unavailable")?;
        let provides_samples = stream_info.dwFlags
            & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32
                | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32)
            != 0;
        let event_generator: IMFMediaEventGenerator = transform.cast()?;
        unsafe {
            transform
                .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)
                .context("hardware H.264 encoder could not be flushed")?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .context("hardware H.264 encoder could not begin streaming")?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .context("hardware H.264 encoder could not start its stream")?;
        }
        let frame_duration_hns = 10_000_000_i64 / i64::from(settings.fps.max(1));
        let mut encoder = Self {
            transform,
            _device_manager: manager,
            event_generator,
            codec_api,
            output_stream_provides_samples: provides_samples,
            output_buffer_size: stream_info.cbSize.max(1_048_576),
            frame_duration_hns,
            next_sample_time_hns: 0,
            events: EncoderEvents {
                needs_input: 0,
                has_output: 0,
            },
            parameter_sets: Vec::new(),
        };
        encoder.wait_for_input(|| false)?;
        Ok(encoder)
    }

    fn encode(
        &mut self,
        texture: &ID3D11Texture2D,
        _captured_at: Instant,
        force_keyframe: bool,
        should_stop: impl Fn() -> bool,
    ) -> Result<Option<Bytes>> {
        self.wait_for_input(&should_stop)?;
        if should_stop() {
            return Ok(None);
        }
        if force_keyframe && let Some(codec_api) = &self.codec_api {
            let _ = set_codec_bool(codec_api, &CODECAPI_AVEncVideoForceKeyFrame, true);
        }
        let sample =
            create_dxgi_sample(texture, self.next_sample_time_hns, self.frame_duration_hns)?;
        self.next_sample_time_hns += self.frame_duration_hns;
        unsafe {
            self.transform
                .ProcessInput(0, &sample, 0)
                .context("hardware H.264 encoder rejected a desktop frame")?;
        }
        self.events.needs_input = self.events.needs_input.saturating_sub(1);
        self.wait_for_output(&should_stop)?;
        if should_stop() {
            return Ok(None);
        }
        let mut encoded = self.read_output()?;
        if encoded.is_empty() {
            return Ok(None);
        }
        encoded = normalize_h264(&encoded)?;
        self.refresh_parameter_sets();
        let is_keyframe = contains_h264_nal_type(&encoded, 5);
        if is_keyframe {
            let current_parameter_sets = extract_parameter_sets(&encoded);
            if !current_parameter_sets.is_empty() {
                self.parameter_sets = current_parameter_sets;
            } else if !self.parameter_sets.is_empty() {
                let mut with_headers =
                    Vec::with_capacity(self.parameter_sets.len() + encoded.len());
                with_headers.extend_from_slice(&self.parameter_sets);
                with_headers.extend_from_slice(&encoded);
                encoded = with_headers;
            }
        }
        Ok(Some(Bytes::from(encoded)))
    }

    fn wait_for_input(&mut self, should_stop: impl Fn() -> bool) -> Result<()> {
        self.wait_for_event(true, should_stop)
    }

    fn wait_for_output(&mut self, should_stop: impl Fn() -> bool) -> Result<()> {
        self.wait_for_event(false, should_stop)
    }

    fn wait_for_event(&mut self, input: bool, should_stop: impl Fn() -> bool) -> Result<()> {
        let started = Instant::now();
        loop {
            self.pump_events()?;
            let available = if input {
                self.events.needs_input > 0
            } else {
                self.events.has_output > 0
            };
            if available || should_stop() {
                return Ok(());
            }
            if started.elapsed() >= ENCODER_EVENT_TIMEOUT {
                bail!(
                    "hardware H.264 encoder timed out waiting for {}",
                    if input { "input" } else { "output" }
                );
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn pump_events(&mut self) -> Result<()> {
        loop {
            let event = match unsafe { self.event_generator.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                Ok(event) => event,
                Err(error) if error.code() == MF_E_NO_EVENTS_AVAILABLE => break,
                Err(error) => {
                    return Err(error).context("hardware H.264 encoder event polling failed");
                }
            };
            let status = unsafe { event.GetStatus() }
                .context("hardware H.264 encoder reported a failed event")?;
            status.ok().context("hardware H.264 encoder event failed")?;
            match unsafe { event.GetType() }
                .context("hardware H.264 encoder event type was unavailable")?
            {
                event_type if event_type == METransformNeedInput.0 as u32 => {
                    self.events.needs_input += 1;
                }
                event_type if event_type == METransformHaveOutput.0 as u32 => {
                    self.events.has_output += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn read_output(&mut self) -> Result<Vec<u8>> {
        let sample = if self.output_stream_provides_samples {
            None
        } else {
            let sample = unsafe { MFCreateSample() }?;
            let buffer = unsafe { MFCreateMemoryBuffer(self.output_buffer_size) }?;
            unsafe { sample.AddBuffer(&buffer) }?;
            Some(sample)
        };
        let mut output = MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: ManuallyDrop::new(sample),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        };
        let process_result = unsafe {
            self.transform
                .ProcessOutput(0, slice::from_mut(&mut output), null_mut())
        };
        self.events.has_output = self.events.has_output.saturating_sub(1);
        let sample = unsafe { ManuallyDrop::take(&mut output.pSample) };
        let events = unsafe { ManuallyDrop::take(&mut output.pEvents) };
        drop(events);
        if let Err(error) = process_result {
            if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT {
                return Ok(Vec::new());
            }
            return Err(error).context("hardware H.264 output could not be read");
        }
        let sample = sample.ok_or_else(|| anyhow!("hardware H.264 encoder returned no sample"))?;
        let buffer = unsafe { sample.ConvertToContiguousBuffer() }
            .context("hardware H.264 output could not be made contiguous")?;
        copy_media_buffer(&buffer)
    }

    fn refresh_parameter_sets(&mut self) {
        let Ok(media_type) = (unsafe { self.transform.GetOutputCurrentType(0) }) else {
            return;
        };
        let Ok(size) = (unsafe { media_type.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) }) else {
            return;
        };
        let mut header = vec![0; size as usize];
        if unsafe { media_type.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut header, None) }.is_ok()
            && let Ok(parameter_sets) = avc_decoder_configuration_to_annex_b(&header)
            && !parameter_sets.is_empty()
        {
            self.parameter_sets = parameter_sets;
        }
    }
}

impl Drop for HardwareH264Encoder {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
        }
    }
}

fn enumerate_hardware_encoder(
    adapter_luid_low: u32,
    adapter_luid_high: i32,
) -> Result<IMFTransform> {
    let mut attributes: Option<IMFAttributes> = None;
    unsafe {
        MFCreateAttributes(&mut attributes, 1)
            .context("Media Foundation encoder attributes could not be created")?;
    }
    let attributes =
        attributes.ok_or_else(|| anyhow!("Media Foundation returned no attributes"))?;
    let luid = u64::from(adapter_luid_low) | ((adapter_luid_high as u32 as u64) << 32);
    unsafe {
        attributes
            .SetUINT64(&MFT_ENUM_ADAPTER_LUID, luid)
            .context("Media Foundation adapter LUID could not be set")?;
    }
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };
    let output = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let flags = MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0);
    let mut activates: *mut Option<IMFActivate> = null_mut();
    let mut count = 0;
    unsafe {
        MFTEnum2(
            MFT_CATEGORY_VIDEO_ENCODER,
            flags,
            Some(&input),
            Some(&output),
            &attributes,
            &mut activates,
            &mut count,
        )
        .context("Media Foundation hardware H.264 enumeration failed")?;
    }
    if count == 0 || activates.is_null() {
        bail!("no hardware H.264 encoder is available for the desktop adapter");
    }
    let slice = unsafe { slice::from_raw_parts_mut(activates, count as usize) };
    let activate = slice.iter().find_map(Clone::clone);
    for item in slice {
        unsafe {
            std::ptr::drop_in_place(item);
        }
    }
    unsafe {
        CoTaskMemFree(Some(activates.cast::<c_void>()));
    }
    let activate =
        activate.ok_or_else(|| anyhow!("Media Foundation returned an empty encoder activation"))?;
    unsafe {
        activate
            .ActivateObject::<IMFTransform>()
            .context("hardware H.264 encoder could not be activated")
    }
}

fn create_video_type(
    subtype: windows::core::GUID,
    geometry: NativeDesktopGeometry,
    settings: NativeVideoSettings,
    encoded: bool,
) -> Result<IMFMediaType> {
    let media_type = unsafe { MFCreateMediaType() }
        .context("Media Foundation video type could not be created")?;
    unsafe {
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, &subtype)?;
        media_type.SetUINT64(
            &MF_MT_FRAME_SIZE,
            (u64::from(geometry.width) << 32) | u64::from(geometry.height),
        )?;
        media_type.SetUINT64(
            &MF_MT_FRAME_RATE,
            (u64::from(settings.fps.max(1)) << 32) | 1,
        )?;
        media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1_u64 << 32) | 1)?;
        media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        if encoded {
            media_type.SetUINT32(
                &MF_MT_AVG_BITRATE,
                settings.bitrate_kbps.saturating_mul(1_000),
            )?;
            media_type.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)?;
            let level = u32::from_str_radix(
                &super::h264_profile_level_id(
                    settings.max_width,
                    settings.max_height,
                    settings.fps,
                    settings.bitrate_kbps,
                )[4..],
                16,
            )
            .context("configured H.264 level is invalid")?;
            media_type.SetUINT32(&MF_MT_MPEG2_LEVEL, level)?;
        }
    }
    Ok(media_type)
}

fn create_dxgi_sample(
    texture: &ID3D11Texture2D,
    sample_time_hns: i64,
    sample_duration_hns: i64,
) -> Result<IMFSample> {
    let buffer = unsafe { MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false) }
        .context("Media Foundation could not wrap the NV12 texture")?;
    let sample = unsafe { MFCreateSample() }
        .context("Media Foundation input sample could not be created")?;
    unsafe {
        sample.AddBuffer(&buffer)?;
        sample.SetSampleTime(sample_time_hns)?;
        sample.SetSampleDuration(sample_duration_hns)?;
    }
    Ok(sample)
}

fn copy_media_buffer(buffer: &IMFMediaBuffer) -> Result<Vec<u8>> {
    let mut bytes = null_mut();
    let mut current_length = 0;
    unsafe {
        buffer
            .Lock(&mut bytes, None, Some(&mut current_length))
            .context("hardware H.264 output buffer could not be locked")?;
    }
    let copied = if bytes.is_null() || current_length == 0 {
        Vec::new()
    } else {
        unsafe { slice::from_raw_parts(bytes, current_length as usize) }.to_vec()
    };
    unsafe {
        buffer
            .Unlock()
            .context("hardware H.264 output buffer could not be unlocked")?;
    }
    Ok(copied)
}

fn set_codec_bool(
    codec_api: &ICodecAPI,
    key: &windows::core::GUID,
    value: bool,
) -> windows::core::Result<()> {
    let mut variant = VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_BOOL,
                Anonymous: VARIANT_0_0_0 {
                    boolVal: if value {
                        VARIANT_TRUE
                    } else {
                        Default::default()
                    },
                },
                ..Default::default()
            }),
        },
    };
    let result = unsafe { codec_api.SetValue(key, &variant) };
    unsafe {
        ManuallyDrop::drop(&mut variant.Anonymous.Anonymous);
    }
    result
}

fn set_codec_u32(
    codec_api: &ICodecAPI,
    key: &windows::core::GUID,
    value: u32,
) -> windows::core::Result<()> {
    let mut variant = VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_UI4,
                Anonymous: VARIANT_0_0_0 { ulVal: value },
                ..Default::default()
            }),
        },
    };
    let result = unsafe { codec_api.SetValue(key, &variant) };
    unsafe {
        ManuallyDrop::drop(&mut variant.Anonymous.Anonymous);
    }
    result
}

fn normalize_h264(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() || starts_with_annex_b(input) {
        return Ok(input.to_vec());
    }
    let mut output = Vec::with_capacity(input.len() + 16);
    let mut offset = 0;
    while offset + 4 <= input.len() {
        let length = u32::from_be_bytes(input[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        if length == 0 || offset + length > input.len() {
            output.clear();
            break;
        }
        output.extend_from_slice(&[0, 0, 0, 1]);
        output.extend_from_slice(&input[offset..offset + length]);
        offset += length;
    }
    if !output.is_empty() && offset == input.len() {
        return Ok(output);
    }
    if input[0] & 0x1f <= 23 {
        let mut single_nal = Vec::with_capacity(input.len() + 4);
        single_nal.extend_from_slice(&[0, 0, 0, 1]);
        single_nal.extend_from_slice(input);
        return Ok(single_nal);
    }
    bail!("hardware H.264 encoder returned an unknown byte-stream format")
}

fn starts_with_annex_b(input: &[u8]) -> bool {
    input.starts_with(&[0, 0, 1]) || input.starts_with(&[0, 0, 0, 1])
}

fn annex_b_nal_units(input: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= input.len() {
        let start_len = if input[index..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if input[index..].starts_with(&[0, 0, 1]) {
            3
        } else {
            index += 1;
            continue;
        };
        starts.push((index, start_len));
        index += start_len;
    }
    starts
        .iter()
        .enumerate()
        .filter_map(|(position, (start, start_len))| {
            let nal_start = start + start_len;
            let nal_end = starts
                .get(position + 1)
                .map_or(input.len(), |(next, _)| *next);
            (nal_start < nal_end).then_some(&input[nal_start..nal_end])
        })
        .collect()
}

fn contains_h264_nal_type(input: &[u8], nal_type: u8) -> bool {
    annex_b_nal_units(input)
        .iter()
        .any(|nal| nal.first().is_some_and(|byte| byte & 0x1f == nal_type))
}

fn extract_parameter_sets(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for nal in annex_b_nal_units(input) {
        if nal.first().is_some_and(|byte| matches!(byte & 0x1f, 7 | 8)) {
            output.extend_from_slice(&[0, 0, 0, 1]);
            output.extend_from_slice(nal);
        }
    }
    output
}

fn avc_decoder_configuration_to_annex_b(input: &[u8]) -> Result<Vec<u8>> {
    if starts_with_annex_b(input) {
        return Ok(extract_parameter_sets(input));
    }
    if input.len() < 7 || input[0] != 1 {
        return Ok(Vec::new());
    }
    let mut offset = 5;
    let sps_count = usize::from(input[offset] & 0x1f);
    offset += 1;
    let mut output = Vec::new();
    for _ in 0..sps_count {
        append_avc_parameter_set(input, &mut offset, &mut output)?;
    }
    if offset >= input.len() {
        return Ok(output);
    }
    let pps_count = usize::from(input[offset]);
    offset += 1;
    for _ in 0..pps_count {
        append_avc_parameter_set(input, &mut offset, &mut output)?;
    }
    Ok(output)
}

fn append_avc_parameter_set(input: &[u8], offset: &mut usize, output: &mut Vec<u8>) -> Result<()> {
    if *offset + 2 > input.len() {
        bail!("H.264 sequence header is truncated");
    }
    let length = u16::from_be_bytes(input[*offset..*offset + 2].try_into().unwrap()) as usize;
    *offset += 2;
    if length == 0 || *offset + length > input.len() {
        bail!("H.264 sequence header has an invalid parameter-set length");
    }
    output.extend_from_slice(&[0, 0, 0, 1]);
    output.extend_from_slice(&input[*offset..*offset + length]);
    *offset += length;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        GpuVideoPipeline, NativeVideoSettings, WindowsMediaRuntime,
        avc_decoder_configuration_to_annex_b, contains_h264_nal_type, extract_parameter_sets,
        normalize_h264,
    };

    #[test]
    #[ignore = "requires an interactive Windows desktop and a hardware H.264 encoder"]
    fn hardware_pipeline_initializes_on_the_current_desktop() {
        let _runtime = WindowsMediaRuntime::new().unwrap();
        let pipeline =
            GpuVideoPipeline::new(NativeVideoSettings::new(30, 4_000, 1_920, 1_080)).unwrap();

        assert!(!pipeline.outputs.is_empty());
        assert!(pipeline.geometry.width <= 1_920);
        assert!(pipeline.geometry.height <= 1_080);
    }

    #[test]
    fn converts_length_prefixed_h264_to_annex_b() {
        let converted = normalize_h264(&[0, 0, 0, 2, 0x65, 0xaa, 0, 0, 0, 2, 0x41, 0xbb]).unwrap();
        assert_eq!(converted, [0, 0, 0, 1, 0x65, 0xaa, 0, 0, 0, 1, 0x41, 0xbb]);
        assert!(contains_h264_nal_type(&converted, 5));
    }

    #[test]
    fn extracts_annex_b_parameter_sets() {
        let stream = [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 1, 0x65, 4];
        assert_eq!(
            extract_parameter_sets(&stream),
            [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 0, 1, 0x68, 3]
        );
    }

    #[test]
    fn converts_avc_decoder_configuration_parameter_sets() {
        let avcc = [
            1, 0x42, 0, 0x28, 0xff, 0xe1, 0, 3, 0x67, 1, 2, 1, 0, 2, 0x68, 3,
        ];
        assert_eq!(
            avc_decoder_configuration_to_annex_b(&avcc).unwrap(),
            [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 0, 1, 0x68, 3]
        );
    }
}
