mod display;
mod native;
mod session_host;

use std::net::IpAddr;

use axum::extract::ws::WebSocket;
use serde::Serialize;
use thiserror::Error;

use crate::config::{DesktopConfig, DesktopIceServerConfig};

pub(crate) use display::{
    DesktopResolutionError, DesktopResolutionResponse, DesktopScreenResponse,
    detect_desktop_resolutions, detect_desktop_screens, set_desktop_resolution,
};
pub(crate) use native::{
    InputDesktop, NativeControllerLeaseState, NativeDesktopCommand, NativeDesktopCursor,
    NativeDesktopFrame, NativeDesktopGeometry, NativeDesktopPixels, NativeInputController,
    fit_native_desktop_geometry, native_cursor_style, native_desktop_geometry,
    native_input_controller, scale_native_desktop_screens,
};
pub(crate) use session_host::{NativeSessionBridge, run_native_session_host};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DesktopInfoResponse {
    pub label: String,
    pub view_only: bool,
    pub websocket_href: String,
    pub screens: Vec<DesktopScreenResponse>,
    pub resolutions: Vec<DesktopResolutionResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub(crate) struct DesktopSessionConfig {
    pub max_fps: u16,
    pub bitrate_kbps: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub ice_servers: Vec<DesktopIceServerConfig>,
}

#[derive(Debug, Error)]
pub(crate) enum DesktopError {
    #[error("desktop streaming is only supported on Windows")]
    UnsupportedPlatform,
}

impl TryFrom<&DesktopConfig> for DesktopSessionConfig {
    type Error = DesktopError;

    fn try_from(config: &DesktopConfig) -> Result<Self, Self::Error> {
        if !cfg!(windows) {
            return Err(DesktopError::UnsupportedPlatform);
        }

        Ok(Self {
            max_fps: config.max_fps,
            bitrate_kbps: config.bitrate_kbps,
            max_width: config.max_width,
            max_height: config.max_height,
            ice_servers: config.ice_servers.clone(),
        })
    }
}

pub(crate) fn desktop_info_response(
    config: &DesktopConfig,
    websocket_href: String,
) -> DesktopInfoResponse {
    DesktopInfoResponse {
        label: config.label.clone(),
        view_only: config.view_only,
        websocket_href,
        screens: desktop_screens(config),
        resolutions: detect_desktop_resolutions(),
    }
}

pub(crate) fn desktop_screens(config: &DesktopConfig) -> Vec<DesktopScreenResponse> {
    let screens = detect_desktop_screens();
    let Ok(source) = native_desktop_geometry() else {
        return screens;
    };
    let output = fit_native_desktop_geometry(source, config.max_width, config.max_height);
    scale_native_desktop_screens(screens, source, output)
}

pub(crate) async fn desktop_websocket_session(
    socket: WebSocket,
    session_config: DesktopSessionConfig,
    view_only: bool,
    peer_ip: Option<IpAddr>,
) {
    crate::desktop_webrtc::desktop_websocket_session(socket, session_config, view_only, peer_ip)
        .await
}
