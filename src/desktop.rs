use std::{
    net::{IpAddr, TcpListener},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use axum::extract::ws::{Message, WebSocket};
use serde::Serialize;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::{Child, Command},
    sync::Mutex,
    time::{sleep, timeout},
};
use tracing::{debug, info, warn};

use crate::config::{DesktopConfig, DesktopMode, ManagedDesktopProvider};

const DESKTOP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MANAGED_DESKTOP_START_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, Serialize)]
pub struct DesktopInfoResponse {
    pub label: String,
    pub enabled: bool,
    pub mode: DesktopMode,
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
    pub host: String,
    pub port: u16,
    pub managed: bool,
}

#[derive(Debug, Error)]
pub enum DesktopError {
    #[error("managed desktop mode is only supported on Windows")]
    UnsupportedManagedPlatform,
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
            host: config.vnc_host.clone(),
            port: config.vnc_port,
            managed: false,
        }
    }

    fn managed(port: u16) -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port,
            managed: true,
        }
    }
}

impl ManagedDesktopManager {
    pub async fn target_for(&self, config: &DesktopConfig) -> Result<DesktopTarget, DesktopError> {
        match config.mode {
            DesktopMode::External => Ok(DesktopTarget::external(config)),
            DesktopMode::Managed => self.ensure_ultravnc(config).await,
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
                return Ok(existing.target());
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
        let target = managed.target();
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

    fn target(&self) -> DesktopTarget {
        DesktopTarget::managed(self.port)
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

pub async fn desktop_websocket_session(mut socket: WebSocket, target: DesktopTarget) {
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

    debug!(%address, "desktop VNC bridge connected");
    let (mut tcp_reader, mut tcp_writer) = stream.into_split();
    let mut buffer = [0_u8; 16 * 1024];

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
    use super::{RawDesktopScreen, display_number, normalize_desktop_screens};

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
