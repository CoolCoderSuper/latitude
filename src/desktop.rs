mod display;
mod managed;
mod native;
mod vnc;

use std::{net::IpAddr, path::PathBuf};

use axum::extract::ws::WebSocket;
use serde::Serialize;
use thiserror::Error;

use crate::config::{DesktopConfig, DesktopIceServerConfig, DesktopMode};

pub use display::{
    DesktopResolutionError, DesktopResolutionResponse, DesktopScreenResponse,
    detect_desktop_resolutions, detect_desktop_screens, set_desktop_resolution,
};
pub use managed::ManagedDesktopManager;
pub(crate) use native::{
    NativeControllerLeaseState, NativeDesktopCapture, NativeDesktopCommand, NativeDesktopCursor,
    NativeDesktopFrame, NativeDesktopGeometry, NativeInputController, native_desktop_geometry,
    native_input_controller,
};

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
pub struct DesktopTarget {
    pub protocol: DesktopProtocol,
    pub host: String,
    pub port: u16,
    pub managed: bool,
    pub native_max_fps: u16,
    pub native_bitrate_kbps: u32,
    pub native_ice_servers: Vec<DesktopIceServerConfig>,
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

impl DesktopTarget {
    fn external(config: &DesktopConfig) -> Self {
        Self {
            protocol: DesktopProtocol::Rfb,
            host: config.vnc_host.clone(),
            port: config.vnc_port,
            managed: false,
            native_max_fps: config.native_max_fps,
            native_bitrate_kbps: config.native_bitrate_kbps,
            native_ice_servers: config.native_ice_servers.clone(),
        }
    }

    fn managed(config: &DesktopConfig, port: u16) -> Self {
        Self {
            protocol: DesktopProtocol::Rfb,
            host: "127.0.0.1".to_string(),
            port,
            managed: true,
            native_max_fps: config.native_max_fps,
            native_bitrate_kbps: config.native_bitrate_kbps,
            native_ice_servers: config.native_ice_servers.clone(),
        }
    }

    fn native(config: &DesktopConfig) -> Self {
        Self {
            protocol: DesktopProtocol::LatitudeNative,
            host: String::new(),
            port: 0,
            managed: false,
            native_max_fps: config.native_max_fps,
            native_bitrate_kbps: config.native_bitrate_kbps,
            native_ice_servers: config.native_ice_servers.clone(),
        }
    }
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

pub async fn desktop_websocket_session(
    socket: WebSocket,
    target: DesktopTarget,
    view_only: bool,
    peer_ip: Option<IpAddr>,
) {
    match target.protocol {
        DesktopProtocol::Rfb => vnc::desktop_websocket_session(socket, target).await,
        DesktopProtocol::LatitudeNative => {
            crate::desktop_webrtc::native_desktop_websocket_session(
                socket, target, view_only, peer_ip,
            )
            .await
        }
    }
}
