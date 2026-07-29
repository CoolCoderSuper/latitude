use std::{
    collections::BTreeSet,
    net::{IpAddr, TcpListener},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use axum::extract::ws::{Message, WebSocket};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::{Child, Command},
    sync::{Mutex, mpsc},
    time::{MissedTickBehavior, sleep, timeout},
};
use tracing::{debug, info, warn};

use crate::config::{DesktopConfig, DesktopMode, ManagedDesktopProvider};

const DESKTOP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MANAGED_DESKTOP_START_TIMEOUT: Duration = Duration::from_secs(8);
const DESKTOP_BRIDGE_BUFFER_SIZE: usize = 64 * 1024;
const NATIVE_DESKTOP_MAX_TEXT_UNITS: usize = 4096;

#[derive(Clone, Debug, Serialize)]
pub struct DesktopInfoResponse {
    pub label: String,
    pub enabled: bool,
    pub mode: DesktopMode,
    pub protocol: DesktopProtocol,
    pub managed: bool,
    pub host: String,
    pub port: u16,
    pub view_only: bool,
    pub websocket_href: String,
    pub screens: Vec<DesktopScreenResponse>,
    pub resolutions: Vec<DesktopResolutionResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DesktopScreenResponse {
    pub id: String,
    pub label: String,
    pub title: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DesktopResolutionResponse {
    pub width: u32,
    pub height: u32,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DesktopResolutionChangeResponse {
    pub ok: bool,
    pub width: u32,
    pub height: u32,
    pub screens: Vec<DesktopScreenResponse>,
    pub resolutions: Vec<DesktopResolutionResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DesktopTarget {
    pub protocol: DesktopProtocol,
    pub host: String,
    pub port: u16,
    pub managed: bool,
    pub native_max_fps: u16,
    pub native_jpeg_quality: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProtocol {
    Rfb,
    LatitudeNative,
}

impl DesktopProtocol {
    pub fn for_mode(mode: DesktopMode) -> Self {
        match mode {
            DesktopMode::External | DesktopMode::Managed => Self::Rfb,
            DesktopMode::Native => Self::LatitudeNative,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rfb => "rfb",
            Self::LatitudeNative => "latitude_native",
        }
    }
}

#[derive(Debug, Error)]
pub enum DesktopError {
    #[error("managed desktop mode is only supported on Windows")]
    UnsupportedManagedPlatform,
    #[error("native desktop mode is only supported on Windows")]
    UnsupportedNativePlatform,
    #[error("managed desktop executable path is empty")]
    EmptyManagedExecutable,
    #[error("managed desktop executable was not found at {0}")]
    MissingManagedExecutable(PathBuf),
    #[error("managed desktop executable has no parent directory: {0}")]
    MissingManagedExecutableParent(PathBuf),
    #[error("UltraVNC exited before opening its VNC listener: {0}")]
    ManagedProcessExited(String),
    #[error(
        "UltraVNC did not open 127.0.0.1:{port} within {timeout_seconds}s; last error: {last_error}"
    )]
    ManagedStartupTimedOut {
        port: u16,
        timeout_seconds: u64,
        last_error: String,
    },
    #[error("desktop manager I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum DesktopResolutionError {
    #[error("changing desktop resolution is only supported on Windows")]
    #[cfg_attr(windows, allow(dead_code))]
    UnsupportedPlatform,
    #[error("desktop resolution must be between 640x480 and 7680x4320")]
    InvalidDimensions,
    #[error("desktop screen '{0}' is not a Windows display id")]
    InvalidScreenId(String),
    #[error("current display settings could not be read")]
    CurrentSettingsUnavailable,
    #[error("Windows rejected resolution {width}x{height}: {code}")]
    ChangeFailed { width: u32, height: u32, code: i32 },
}

#[derive(Debug, Default)]
pub struct ManagedDesktopManager {
    process: Mutex<Option<ManagedDesktopProcess>>,
}

pub fn desktop_info_response(
    config: &DesktopConfig,
    target: &DesktopTarget,
    websocket_href: String,
) -> DesktopInfoResponse {
    DesktopInfoResponse {
        label: config.label.clone(),
        enabled: config.enabled,
        mode: config.mode,
        protocol: target.protocol,
        managed: target.managed,
        host: target.host.clone(),
        port: target.port,
        view_only: config.view_only,
        websocket_href,
        screens: detect_desktop_screens(),
        resolutions: detect_desktop_resolutions(),
    }
}

pub fn detect_desktop_screens() -> Vec<DesktopScreenResponse> {
    platform_desktop_screens()
}

pub fn detect_desktop_resolutions() -> Vec<DesktopResolutionResponse> {
    platform_desktop_resolutions(None)
}

pub fn set_desktop_resolution(
    screen_id: Option<&str>,
    width: u32,
    height: u32,
) -> Result<DesktopResolutionChangeResponse, DesktopResolutionError> {
    if !(640..=7680).contains(&width) || !(480..=4320).contains(&height) {
        return Err(DesktopResolutionError::InvalidDimensions);
    }

    platform_set_desktop_resolution(screen_id, width, height)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawDesktopScreen {
    device: String,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    primary: bool,
}

fn normalize_desktop_screens(mut screens: Vec<RawDesktopScreen>) -> Vec<DesktopScreenResponse> {
    screens.retain(|screen| screen.right > screen.left && screen.bottom > screen.top);
    if screens.len() < 2 {
        return Vec::new();
    }

    let min_x = screens.iter().map(|screen| screen.left).min().unwrap_or(0);
    let min_y = screens.iter().map(|screen| screen.top).min().unwrap_or(0);
    screens.sort_by_key(|screen| {
        (
            display_number(&screen.device).unwrap_or(u32::MAX),
            screen.left,
            screen.top,
        )
    });

    screens
        .into_iter()
        .enumerate()
        .map(|(index, screen)| {
            let display_number = display_number(&screen.device);
            let label = (index + 1).to_string();
            let title = display_number
                .map(|number| format!("Screen {label} (DISPLAY{number})"))
                .unwrap_or_else(|| format!("Screen {label}"));
            DesktopScreenResponse {
                id: display_number
                    .map(|number| format!("display-{number}"))
                    .unwrap_or_else(|| format!("display-{}", index + 1)),
                label,
                title,
                x: (screen.left - min_x).max(0) as u32,
                y: (screen.top - min_y).max(0) as u32,
                width: (screen.right - screen.left) as u32,
                height: (screen.bottom - screen.top) as u32,
                primary: screen.primary,
            }
        })
        .collect()
}

fn display_number(device: &str) -> Option<u32> {
    let suffix = device.strip_prefix(r"\\.\DISPLAY")?;
    suffix.parse::<u32>().ok()
}

impl DesktopTarget {
    fn external(config: &DesktopConfig) -> Self {
        Self {
            protocol: DesktopProtocol::Rfb,
            host: config.vnc_host.clone(),
            port: config.vnc_port,
            managed: false,
            native_max_fps: config.native_max_fps,
            native_jpeg_quality: config.native_jpeg_quality,
        }
    }

    fn managed(config: &DesktopConfig, port: u16) -> Self {
        Self {
            protocol: DesktopProtocol::Rfb,
            host: "127.0.0.1".to_string(),
            port,
            managed: true,
            native_max_fps: config.native_max_fps,
            native_jpeg_quality: config.native_jpeg_quality,
        }
    }

    fn native(config: &DesktopConfig) -> Self {
        Self {
            protocol: DesktopProtocol::LatitudeNative,
            host: String::new(),
            port: 0,
            managed: false,
            native_max_fps: config.native_max_fps,
            native_jpeg_quality: config.native_jpeg_quality,
        }
    }
}

impl ManagedDesktopManager {
    pub async fn target_for(&self, config: &DesktopConfig) -> Result<DesktopTarget, DesktopError> {
        match config.mode {
            DesktopMode::External => Ok(DesktopTarget::external(config)),
            DesktopMode::Managed => self.ensure_ultravnc(config).await,
            DesktopMode::Native => {
                if !cfg!(windows) {
                    return Err(DesktopError::UnsupportedNativePlatform);
                }
                Ok(DesktopTarget::native(config))
            }
        }
    }

    async fn ensure_ultravnc(&self, config: &DesktopConfig) -> Result<DesktopTarget, DesktopError> {
        if !cfg!(windows) {
            return Err(DesktopError::UnsupportedManagedPlatform);
        }

        let executable = resolve_managed_executable(&config.managed_executable)?;
        let mut process = self.process.lock().await;
        if let Some(existing) = process.as_mut()
            && existing.matches(config.managed_provider, &executable, config.view_only)
        {
            if existing.is_running().await? {
                return Ok(existing.target(config));
            }
        }

        if let Some(mut existing) = process.take() {
            existing.stop();
        }

        let port = available_loopback_port()?;
        let parent = executable
            .parent()
            .ok_or_else(|| DesktopError::MissingManagedExecutableParent(executable.clone()))?;
        write_ultravnc_ini(parent, port, config.view_only).await?;

        let mut child = Command::new(&executable)
            .arg("-multi")
            .arg("-run")
            .current_dir(parent)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        wait_for_managed_listener(&mut child, port).await?;

        let managed = ManagedDesktopProcess {
            child,
            executable,
            provider: config.managed_provider,
            port,
            view_only: config.view_only,
        };
        let target = managed.target(config);
        *process = Some(managed);
        info!(port = target.port, "managed UltraVNC desktop started");
        Ok(target)
    }
}

#[derive(Debug)]
struct ManagedDesktopProcess {
    child: Child,
    executable: PathBuf,
    provider: ManagedDesktopProvider,
    port: u16,
    view_only: bool,
}

impl ManagedDesktopProcess {
    fn matches(
        &self,
        provider: ManagedDesktopProvider,
        executable: &Path,
        view_only: bool,
    ) -> bool {
        self.provider == provider && self.executable == executable && self.view_only == view_only
    }

    fn target(&self, config: &DesktopConfig) -> DesktopTarget {
        DesktopTarget::managed(config, self.port)
    }

    async fn is_running(&mut self) -> Result<bool, DesktopError> {
        if self.child.try_wait()?.is_none() {
            return Ok(true);
        }

        Ok(managed_listener_is_open(self.port).await)
    }

    fn stop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl Drop for ManagedDesktopProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn desktop_websocket_session(socket: WebSocket, target: DesktopTarget, view_only: bool) {
    match target.protocol {
        DesktopProtocol::Rfb => vnc_desktop_websocket_session(socket, target).await,
        DesktopProtocol::LatitudeNative => {
            native_desktop_websocket_session(socket, target, view_only).await
        }
    }
}

async fn vnc_desktop_websocket_session(mut socket: WebSocket, target: DesktopTarget) {
    let address = desktop_vnc_address(&target.host, target.port);
    let connected_at = Instant::now();
    let stream = match timeout(DESKTOP_CONNECT_TIMEOUT, TcpStream::connect(&address)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            warn!(%address, %error, "desktop VNC connection failed");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
        Err(_) => {
            warn!(%address, "desktop VNC connection timed out");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    if let Err(error) = stream.set_nodelay(true) {
        warn!(%address, %error, "desktop VNC bridge could not disable TCP buffering");
    }

    debug!(%address, "desktop VNC bridge connected");
    let (mut tcp_reader, mut tcp_writer) = stream.into_split();
    let mut buffer = vec![0_u8; DESKTOP_BRIDGE_BUFFER_SIZE];

    loop {
        tokio::select! {
            read = tcp_reader.read(&mut buffer) => {
                match read {
                    Ok(0) => break,
                    Ok(count) => {
                        if socket
                            .send(Message::Binary(buffer[..count].to_vec().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        warn!(%address, %error, "desktop VNC read failed");
                        break;
                    }
                }
            }
            message = socket.recv() => {
                let Some(message) = message else {
                    break;
                };
                let Ok(message) = message else {
                    break;
                };

                match message {
                    Message::Binary(bytes) => {
                        if tcp_writer.write_all(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Message::Text(text) => {
                        if tcp_writer.write_all(text.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }

    debug!(
        %address,
        duration_ms = connected_at.elapsed().as_millis(),
        "desktop VNC bridge closed"
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct NativeDesktopGeometry {
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug)]
struct NativeDesktopFrame {
    geometry: NativeDesktopGeometry,
    cursor: NativeDesktopCursor,
    jpeg: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum NativeDesktopCursor {
    Default,
    Text,
    Pointer,
    Wait,
    Progress,
    Crosshair,
    Help,
    NotAllowed,
    Move,
    NsResize,
    EwResize,
    NwseResize,
    NeswResize,
    NResize,
    None,
}

#[derive(Debug, Error)]
enum NativeDesktopError {
    #[cfg(not(windows))]
    #[error("native desktop capture is only supported on Windows")]
    UnsupportedPlatform,
    #[error("Windows desktop operation failed: {0}")]
    WindowsApi(&'static str),
    #[error("native desktop frame encoding failed: {0}")]
    Encode(#[from] image::ImageError),
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NativeDesktopCommand {
    Pointer {
        x: f64,
        y: f64,
        #[serde(default)]
        buttons: u8,
    },
    Wheel {
        #[serde(default)]
        delta_x: i32,
        #[serde(default)]
        delta_y: i32,
    },
    Key {
        vk: u16,
        down: bool,
        #[serde(default)]
        extended: bool,
    },
    Text {
        text: String,
    },
    ReleaseKeys,
    Refresh,
}

#[derive(Debug)]
struct NativeInputState {
    x: f64,
    y: f64,
    buttons: u8,
    keys: BTreeSet<(u16, bool)>,
}

impl Default for NativeInputState {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            buttons: 0,
            keys: BTreeSet::new(),
        }
    }
}

async fn native_desktop_websocket_session(
    mut socket: WebSocket,
    target: DesktopTarget,
    view_only: bool,
) {
    let connected_at = Instant::now();
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(target.native_max_fps.max(1)));
    let jpeg_quality = target.native_jpeg_quality;
    let (frame_tx, mut frame_rx) =
        mpsc::channel::<Result<NativeDesktopFrame, NativeDesktopError>>(1);
    let capture_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(frame_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            if frame_tx.capacity() == 0 {
                continue;
            }

            let frame = match tokio::task::spawn_blocking(move || native_capture_jpeg(jpeg_quality))
                .await
            {
                Ok(frame) => frame,
                Err(error) => Err(NativeDesktopError::Message(format!(
                    "native desktop capture task failed: {error}"
                ))),
            };
            let failed = frame.is_err();
            if frame_tx.send(frame).await.is_err() || failed {
                break;
            }
        }
    });

    let mut input_state = NativeInputState::default();
    let mut last_geometry = None;
    let mut last_cursor = None;

    loop {
        tokio::select! {
            frame = frame_rx.recv() => {
                let Some(frame) = frame else {
                    break;
                };
                let frame = match frame {
                    Ok(frame) => frame,
                    Err(error) => {
                        let message = serde_json::json!({
                            "type": "error",
                            "message": error.to_string(),
                        });
                        let _ = socket.send(Message::Text(message.to_string().into())).await;
                        break;
                    }
                };

                if last_geometry != Some(frame.geometry) {
                    let hello = serde_json::json!({
                        "type": "hello",
                        "protocol": DesktopProtocol::LatitudeNative,
                        "encoding": "jpeg",
                        "origin_x": frame.geometry.origin_x,
                        "origin_y": frame.geometry.origin_y,
                        "width": frame.geometry.width,
                        "height": frame.geometry.height,
                        "view_only": view_only,
                    });
                    if socket
                        .send(Message::Text(hello.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    last_geometry = Some(frame.geometry);
                }

                if last_cursor != Some(frame.cursor) {
                    let cursor = serde_json::json!({
                        "type": "cursor",
                        "cursor": frame.cursor,
                    });
                    if socket
                        .send(Message::Text(cursor.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    last_cursor = Some(frame.cursor);
                }

                if socket
                    .send(Message::Binary(frame.jpeg.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            message = socket.recv() => {
                let Some(message) = message else {
                    break;
                };
                let Ok(message) = message else {
                    break;
                };

                match message {
                    Message::Text(text) => {
                        let command = match serde_json::from_slice::<NativeDesktopCommand>(
                            text.as_bytes(),
                        ) {
                            Ok(command) => command,
                            Err(error) => {
                                debug!(%error, "native desktop command was rejected");
                                continue;
                            }
                        };
                        if view_only && !matches!(command, NativeDesktopCommand::Refresh) {
                            continue;
                        }
                        if let Err(error) = apply_native_desktop_command(command, &mut input_state) {
                            warn!(%error, "native desktop input failed");
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }

    capture_task.abort();
    if input_state.buttons != 0 {
        let _ = apply_native_desktop_command(
            NativeDesktopCommand::Pointer {
                x: input_state.x,
                y: input_state.y,
                buttons: 0,
            },
            &mut input_state,
        );
    }
    if !input_state.keys.is_empty() {
        let _ = apply_native_desktop_command(NativeDesktopCommand::ReleaseKeys, &mut input_state);
    }
    debug!(
        duration_ms = connected_at.elapsed().as_millis(),
        "native desktop bridge closed"
    );
}

#[cfg(windows)]
fn native_capture_jpeg(quality: u8) -> Result<NativeDesktopFrame, NativeDesktopError> {
    use std::{mem::size_of, ptr::null_mut, slice};

    use image::{ExtendedColorType, codecs::jpeg::JpegEncoder};
    use windows_sys::Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC,
            CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HGDIOBJ, RGBQUAD,
            ReleaseDC, SRCCOPY, SelectObject,
        },
        UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
        },
    };

    let origin_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let origin_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if width <= 0 || height <= 0 {
        return Err(NativeDesktopError::WindowsApi(
            "virtual desktop dimensions are unavailable",
        ));
    }

    let screen_dc = unsafe { GetDC(null_mut()) };
    if screen_dc.is_null() {
        return Err(NativeDesktopError::WindowsApi(
            "screen device context could not be opened",
        ));
    }
    let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
    if memory_dc.is_null() {
        unsafe {
            ReleaseDC(null_mut(), screen_dc);
        }
        return Err(NativeDesktopError::WindowsApi(
            "capture device context could not be created",
        ));
    }

    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: (i64::from(width) * i64::from(height) * 4) as u32,
            ..BITMAPINFOHEADER::default()
        },
        bmiColors: [RGBQUAD::default()],
    };
    let mut bits = null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            screen_dc,
            &bitmap_info,
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        )
    };
    if bitmap.is_null() || bits.is_null() {
        unsafe {
            DeleteDC(memory_dc);
            ReleaseDC(null_mut(), screen_dc);
        }
        return Err(NativeDesktopError::WindowsApi(
            "capture bitmap could not be created",
        ));
    }

    let previous = unsafe { SelectObject(memory_dc, bitmap as HGDIOBJ) };
    let copied = unsafe {
        BitBlt(
            memory_dc,
            0,
            0,
            width,
            height,
            screen_dc,
            origin_x,
            origin_y,
            SRCCOPY | CAPTUREBLT,
        )
    };
    let byte_len = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
    if copied != 0 {
        let bgra = unsafe { slice::from_raw_parts(bits.cast::<u8>(), byte_len) };
        for pixel in bgra.chunks_exact(4) {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }

    unsafe {
        if !previous.is_null() {
            SelectObject(memory_dc, previous);
        }
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(memory_dc);
        ReleaseDC(null_mut(), screen_dc);
    }

    if copied == 0 {
        return Err(NativeDesktopError::WindowsApi(
            "desktop pixels could not be copied",
        ));
    }

    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, quality).encode(
        &rgb,
        width as u32,
        height as u32,
        ExtendedColorType::Rgb8,
    )?;

    Ok(NativeDesktopFrame {
        geometry: NativeDesktopGeometry {
            origin_x,
            origin_y,
            width: width as u32,
            height: height as u32,
        },
        cursor: native_cursor_style(),
        jpeg,
    })
}

#[cfg(windows)]
fn native_cursor_style() -> NativeDesktopCursor {
    use std::{mem::size_of, ptr::null_mut};

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CURSOR_SHOWING, CURSORINFO, GetCursorInfo, IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND,
        IDC_HELP, IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE,
        IDC_SIZEWE, IDC_UPARROW, IDC_WAIT, LoadCursorW,
    };

    let mut info = CURSORINFO {
        cbSize: size_of::<CURSORINFO>() as u32,
        ..CURSORINFO::default()
    };
    if unsafe { GetCursorInfo(&mut info) } == 0 {
        return NativeDesktopCursor::Default;
    }
    if info.flags & CURSOR_SHOWING == 0 {
        return NativeDesktopCursor::None;
    }

    let cursor = info.hCursor;
    for (resource, style) in [
        (IDC_ARROW, NativeDesktopCursor::Default),
        (IDC_IBEAM, NativeDesktopCursor::Text),
        (IDC_HAND, NativeDesktopCursor::Pointer),
        (IDC_WAIT, NativeDesktopCursor::Wait),
        (IDC_APPSTARTING, NativeDesktopCursor::Progress),
        (IDC_CROSS, NativeDesktopCursor::Crosshair),
        (IDC_HELP, NativeDesktopCursor::Help),
        (IDC_NO, NativeDesktopCursor::NotAllowed),
        (IDC_SIZEALL, NativeDesktopCursor::Move),
        (IDC_SIZENS, NativeDesktopCursor::NsResize),
        (IDC_SIZEWE, NativeDesktopCursor::EwResize),
        (IDC_SIZENWSE, NativeDesktopCursor::NwseResize),
        (IDC_SIZENESW, NativeDesktopCursor::NeswResize),
        (IDC_UPARROW, NativeDesktopCursor::NResize),
    ] {
        if cursor == unsafe { LoadCursorW(null_mut(), resource) } {
            return style;
        }
    }

    NativeDesktopCursor::Default
}

#[cfg(not(windows))]
fn native_capture_jpeg(_quality: u8) -> Result<NativeDesktopFrame, NativeDesktopError> {
    Err(NativeDesktopError::UnsupportedPlatform)
}

#[cfg(windows)]
fn apply_native_desktop_command(
    command: NativeDesktopCommand,
    state: &mut NativeInputState,
) -> Result<(), NativeDesktopError> {
    use std::mem::size_of;

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
        MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK,
        MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
    };

    fn mouse_input(flags: u32, x: i32, y: i32, data: u32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: x,
                    dy: y,
                    mouseData: data,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn keyboard_input(vk: u16, scan: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn send(inputs: &[INPUT]) -> Result<(), NativeDesktopError> {
        if inputs.is_empty() {
            return Ok(());
        }
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if sent != inputs.len() as u32 {
            return Err(NativeDesktopError::WindowsApi(
                "Windows rejected one or more input events",
            ));
        }
        Ok(())
    }

    match command {
        NativeDesktopCommand::Pointer { x, y, buttons } => {
            let x = x.clamp(0.0, 1.0);
            let y = y.clamp(0.0, 1.0);
            let buttons = buttons & 0x07;
            let mut inputs = vec![mouse_input(
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                (x * 65_535.0).round() as i32,
                (y * 65_535.0).round() as i32,
                0,
            )];
            for (mask, down, up) in [
                (0x01, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
                (0x02, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
                (0x04, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            ] {
                if state.buttons & mask == 0 && buttons & mask != 0 {
                    inputs.push(mouse_input(down, 0, 0, 0));
                } else if state.buttons & mask != 0 && buttons & mask == 0 {
                    inputs.push(mouse_input(up, 0, 0, 0));
                }
            }
            send(&inputs)?;
            state.x = x;
            state.y = y;
            state.buttons = buttons;
        }
        NativeDesktopCommand::Wheel { delta_x, delta_y } => {
            let mut inputs = Vec::with_capacity(2);
            if delta_y != 0 {
                inputs.push(mouse_input(
                    MOUSEEVENTF_WHEEL,
                    0,
                    0,
                    delta_y.clamp(-1200, 1200) as u32,
                ));
            }
            if delta_x != 0 {
                inputs.push(mouse_input(
                    MOUSEEVENTF_HWHEEL,
                    0,
                    0,
                    delta_x.clamp(-1200, 1200) as u32,
                ));
            }
            send(&inputs)?;
        }
        NativeDesktopCommand::Key { vk, down, extended } => {
            if vk == 0 || vk > u16::from(u8::MAX) {
                return Ok(());
            }
            let mut flags = if down { 0 } else { KEYEVENTF_KEYUP };
            if extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            send(&[keyboard_input(vk, 0, flags)])?;
            if down {
                state.keys.insert((vk, extended));
            } else {
                state.keys.remove(&(vk, extended));
            }
        }
        NativeDesktopCommand::ReleaseKeys => {
            let inputs = state
                .keys
                .iter()
                .map(|(vk, extended)| {
                    let mut flags = KEYEVENTF_KEYUP;
                    if *extended {
                        flags |= KEYEVENTF_EXTENDEDKEY;
                    }
                    keyboard_input(*vk, 0, flags)
                })
                .collect::<Vec<_>>();
            send(&inputs)?;
            state.keys.clear();
        }
        NativeDesktopCommand::Text { text } => {
            let mut inputs = Vec::new();
            for unit in text.encode_utf16().take(NATIVE_DESKTOP_MAX_TEXT_UNITS) {
                inputs.push(keyboard_input(0, unit, KEYEVENTF_UNICODE));
                inputs.push(keyboard_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
            }
            send(&inputs)?;
        }
        NativeDesktopCommand::Refresh => {}
    }

    Ok(())
}

#[cfg(not(windows))]
fn apply_native_desktop_command(
    _command: NativeDesktopCommand,
    _state: &mut NativeInputState,
) -> Result<(), NativeDesktopError> {
    Err(NativeDesktopError::UnsupportedPlatform)
}

fn desktop_vnc_address(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    }
}

fn resolve_managed_executable(path: &Path) -> Result<PathBuf, DesktopError> {
    if path.as_os_str().is_empty() {
        return Err(DesktopError::EmptyManagedExecutable);
    }

    let executable = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    if !executable.is_file() {
        return Err(DesktopError::MissingManagedExecutable(executable));
    }

    Ok(executable)
}

fn available_loopback_port() -> Result<u16, std::io::Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

async fn write_ultravnc_ini(
    parent: &Path,
    port: u16,
    view_only: bool,
) -> Result<(), std::io::Error> {
    let inputs_enabled = if view_only { 0 } else { 1 };
    let ini = format!(
        "\
[admin]\n\
UseRegistry=0\n\
SocketConnect=1\n\
primary=1\n\
secondary=1\n\
PortNumber={port}\n\
AutoPortSelect=0\n\
HTTPConnect=0\n\
HTTPPortNumber=0\n\
InputsEnabled={inputs_enabled}\n\
AllowLoopback=1\n\
LoopbackOnly=1\n\
AuthRequired=0\n\
AuthHosts=+127.0.0.1:+::1:\n\
QuerySetting=0\n\
QueryAccept=1\n\
QueryIfNoLogon=0\n\
ConnectPriority=1\n\
MaxViewerSetting=0\n\
MaxViewers=128\n\
IdleTimeout=0\n\
IdleInputTimeout=0\n\
KeepAliveInterval=5\n\
sendbuffer=8192\n\
LockSetting=0\n\
AllowShutdown=0\n\
AllowProperties=0\n\
DisableTrayIcon=1\n\
RemoveWallpaper=0\n\
\n\
[poll]\n\
PollFullScreen=1\n\
PollForeground=1\n\
PollUnderCursor=1\n\
OnlyPollConsole=0\n\
OnlyPollOnEvent=0\n\
MaxCpu2=100\n\
MaxFPS=60\n\
EnableHook=1\n\
EnableDriver=0\n\
EnableVirtual=0\n\
TurboMode=1\n"
    );

    tokio::fs::write(parent.join("ultravnc.portable"), b"").await?;
    tokio::fs::write(parent.join("ultravnc.ini"), ini).await
}

async fn wait_for_managed_listener(child: &mut Child, port: u16) -> Result<(), DesktopError> {
    let started_at = Instant::now();
    let timeout_seconds = MANAGED_DESKTOP_START_TIMEOUT.as_secs();
    let mut last_error = "listener was not checked".to_string();

    loop {
        if let Some(status) = child.try_wait()? {
            return Err(DesktopError::ManagedProcessExited(status.to_string()));
        }

        if started_at.elapsed() >= MANAGED_DESKTOP_START_TIMEOUT {
            return Err(DesktopError::ManagedStartupTimedOut {
                port,
                timeout_seconds,
                last_error,
            });
        }

        match timeout(
            Duration::from_millis(250),
            TcpStream::connect(("127.0.0.1", port)),
        )
        .await
        {
            Ok(Ok(_)) => return Ok(()),
            Ok(Err(error)) => {
                last_error = error.to_string();
            }
            Err(_) => {
                last_error = "connection attempt timed out".to_string();
            }
        }

        sleep(Duration::from_millis(100)).await;
    }
}

async fn managed_listener_is_open(port: u16) -> bool {
    matches!(
        timeout(
            Duration::from_millis(250),
            TcpStream::connect(("127.0.0.1", port)),
        )
        .await,
        Ok(Ok(_))
    )
}

#[cfg(windows)]
fn platform_desktop_screens() -> Vec<DesktopScreenResponse> {
    use std::{mem::size_of, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::{LPARAM, RECT, TRUE},
        Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
        },
    };
    use windows_sys::core::BOOL;

    const MONITORINFOF_PRIMARY: u32 = 1;

    unsafe extern "system" fn collect_monitor(
        monitor: HMONITOR,
        _dc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let screens = unsafe { &mut *(data as *mut Vec<RawDesktopScreen>) };
        let mut info = MONITORINFOEXW {
            monitorInfo: MONITORINFO {
                cbSize: size_of::<MONITORINFOEXW>() as u32,
                rcMonitor: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                rcWork: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                dwFlags: 0,
            },
            szDevice: [0; 32],
        };

        let ok = unsafe {
            GetMonitorInfoW(
                monitor,
                &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
            )
        };
        if ok == 0 {
            return TRUE;
        }

        let device_len = info
            .szDevice
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(info.szDevice.len());
        let device = String::from_utf16_lossy(&info.szDevice[..device_len]);
        screens.push(RawDesktopScreen {
            device,
            left: info.monitorInfo.rcMonitor.left,
            top: info.monitorInfo.rcMonitor.top,
            right: info.monitorInfo.rcMonitor.right,
            bottom: info.monitorInfo.rcMonitor.bottom,
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        });

        TRUE
    }

    let mut screens = Vec::new();
    let ok = unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null_mut(),
            Some(collect_monitor),
            &mut screens as *mut Vec<RawDesktopScreen> as LPARAM,
        )
    };

    if ok == 0 {
        return Vec::new();
    }

    normalize_desktop_screens(screens)
}

#[cfg(windows)]
fn platform_desktop_resolutions(screen_id: Option<&str>) -> Vec<DesktopResolutionResponse> {
    use std::{mem::size_of, ptr::null};
    use windows_sys::Win32::Graphics::Gdi::{
        DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW,
    };
    use windows_sys::core::PCWSTR;

    unsafe fn current_mode(device: PCWSTR) -> Option<DEVMODEW> {
        let mut mode = DEVMODEW::default();
        mode.dmSize = size_of::<DEVMODEW>() as u16;
        let ok = unsafe { EnumDisplaySettingsW(device, ENUM_CURRENT_SETTINGS, &mut mode) };
        (ok != 0).then_some(mode)
    }

    let device_name = windows_display_device_name(screen_id).ok().flatten();
    let device_wide = device_name.as_ref().map(|name| wide_null(name));
    let device = device_wide
        .as_ref()
        .map(|name| name.as_ptr())
        .unwrap_or_else(null);
    let current = unsafe { current_mode(device) };
    let mut modes = Vec::<(u32, u32)>::new();
    let mut mode_index = 0_u32;

    loop {
        let mut mode = DEVMODEW::default();
        mode.dmSize = size_of::<DEVMODEW>() as u16;
        let ok = unsafe { EnumDisplaySettingsW(device, mode_index, &mut mode) };
        if ok == 0 {
            break;
        }

        if mode.dmPelsWidth >= 640
            && mode.dmPelsHeight >= 480
            && !modes
                .iter()
                .any(|(width, height)| *width == mode.dmPelsWidth && *height == mode.dmPelsHeight)
        {
            modes.push((mode.dmPelsWidth, mode.dmPelsHeight));
        }
        mode_index += 1;
    }

    if let Some(current) = current
        && !modes
            .iter()
            .any(|(width, height)| *width == current.dmPelsWidth && *height == current.dmPelsHeight)
    {
        modes.push((current.dmPelsWidth, current.dmPelsHeight));
    }

    modes.sort_by_key(|(width, height)| ((*width as u64) * (*height as u64), *width, *height));

    modes
        .into_iter()
        .map(|(width, height)| DesktopResolutionResponse {
            width,
            height,
            current: current
                .is_some_and(|mode| mode.dmPelsWidth == width && mode.dmPelsHeight == height),
        })
        .collect()
}

#[cfg(not(windows))]
fn platform_desktop_resolutions(_screen_id: Option<&str>) -> Vec<DesktopResolutionResponse> {
    Vec::new()
}

#[cfg(windows)]
fn platform_set_desktop_resolution(
    screen_id: Option<&str>,
    width: u32,
    height: u32,
) -> Result<DesktopResolutionChangeResponse, DesktopResolutionError> {
    use std::{
        mem::size_of,
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::Graphics::Gdi::{
        CDS_TEST, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_SUCCESSFUL, DM_PELSHEIGHT,
        DM_PELSWIDTH, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW,
    };

    let device_name = windows_display_device_name(screen_id)?;
    let device_wide = device_name.as_ref().map(|name| wide_null(name));
    let device = device_wide
        .as_ref()
        .map(|name| name.as_ptr())
        .unwrap_or_else(null);
    let mut mode = DEVMODEW::default();
    mode.dmSize = size_of::<DEVMODEW>() as u16;

    let current_ok = unsafe { EnumDisplaySettingsW(device, ENUM_CURRENT_SETTINGS, &mut mode) };
    if current_ok == 0 {
        return Err(DesktopResolutionError::CurrentSettingsUnavailable);
    }

    mode.dmPelsWidth = width;
    mode.dmPelsHeight = height;
    mode.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT;

    let test = unsafe { ChangeDisplaySettingsExW(device, &mode, null_mut(), CDS_TEST, null()) };
    if test != DISP_CHANGE_SUCCESSFUL {
        return Err(DesktopResolutionError::ChangeFailed {
            width,
            height,
            code: test,
        });
    }

    let result = unsafe { ChangeDisplaySettingsExW(device, &mode, null_mut(), 0, null()) };
    if result != DISP_CHANGE_SUCCESSFUL {
        return Err(DesktopResolutionError::ChangeFailed {
            width,
            height,
            code: result,
        });
    }

    Ok(DesktopResolutionChangeResponse {
        ok: true,
        width,
        height,
        screens: detect_desktop_screens(),
        resolutions: platform_desktop_resolutions(screen_id),
    })
}

#[cfg(not(windows))]
fn platform_set_desktop_resolution(
    _screen_id: Option<&str>,
    _width: u32,
    _height: u32,
) -> Result<DesktopResolutionChangeResponse, DesktopResolutionError> {
    Err(DesktopResolutionError::UnsupportedPlatform)
}

#[cfg(windows)]
fn windows_display_device_name(
    screen_id: Option<&str>,
) -> Result<Option<String>, DesktopResolutionError> {
    let Some(screen_id) = screen_id
        .map(str::trim)
        .filter(|screen_id| !screen_id.is_empty())
    else {
        return Ok(None);
    };

    if screen_id == "all" {
        return Ok(None);
    }

    let Some(number) = screen_id
        .strip_prefix("display-")
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|number| *number > 0)
    else {
        return Err(DesktopResolutionError::InvalidScreenId(
            screen_id.to_string(),
        ));
    };

    Ok(Some(format!(r"\\.\DISPLAY{number}")))
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(windows))]
fn platform_desktop_screens() -> Vec<DesktopScreenResponse> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::{
        NativeDesktopCommand, RawDesktopScreen, display_number, normalize_desktop_screens,
    };

    #[test]
    fn accepts_native_release_keys_command() {
        let command =
            serde_json::from_str::<NativeDesktopCommand>(r#"{"type":"release_keys"}"#).unwrap();

        assert!(matches!(command, NativeDesktopCommand::ReleaseKeys));
    }

    #[test]
    fn extracts_windows_display_number() {
        assert_eq!(display_number(r"\\.\DISPLAY2"), Some(2));
        assert_eq!(display_number("DISPLAY2"), None);
    }

    #[test]
    fn normalizes_negative_monitor_coordinates_and_uses_friendly_labels() {
        let screens = normalize_desktop_screens(vec![
            RawDesktopScreen {
                device: r"\\.\DISPLAY161".to_string(),
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
                primary: true,
            },
            RawDesktopScreen {
                device: r"\\.\DISPLAY162".to_string(),
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1080,
                primary: false,
            },
        ]);

        assert_eq!(screens.len(), 2);
        assert_eq!(screens[0].label, "1");
        assert_eq!(screens[0].x, 1920);
        assert!(screens[0].primary);
        assert_eq!(screens[1].label, "2");
        assert_eq!(screens[1].x, 0);
        assert!(!screens[1].primary);
        assert_eq!(screens[0].id, "display-161");
        assert_eq!(screens[0].title, "Screen 1 (DISPLAY161)");
    }
}
