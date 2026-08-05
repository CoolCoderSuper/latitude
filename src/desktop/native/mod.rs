mod capture;
mod controller;
mod input;
mod input_desktop;

use std::{
    collections::BTreeSet,
    ops::Deref,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::DesktopScreenResponse;

pub(crate) use capture::{NativeDesktopCapture, native_cursor_style, native_desktop_geometry};
pub(crate) use controller::{
    NativeControllerLeaseState, NativeInputController, native_input_controller,
};
pub(crate) use input::apply_native_desktop_command;
pub(crate) use input_desktop::InputDesktop;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct NativeDesktopGeometry {
    pub(crate) origin_x: i32,
    pub(crate) origin_y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug)]
pub(crate) struct NativeDesktopFrame {
    pub(crate) source_geometry: NativeDesktopGeometry,
    pub(crate) geometry: NativeDesktopGeometry,
    pub(crate) cursor: NativeDesktopCursor,
    pub(crate) captured_at: std::time::Instant,
    pub(crate) bgra: NativeDesktopPixels,
}

pub(crate) struct NativeDesktopPixels {
    bytes: Option<Vec<u8>>,
    pool: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl NativeDesktopPixels {
    const MAX_POOLED_BUFFERS: usize = 3;

    pub(crate) fn new(bytes: Vec<u8>, pool: Arc<Mutex<Vec<Vec<u8>>>>) -> Self {
        Self {
            bytes: Some(bytes),
            pool,
        }
    }
}

impl std::fmt::Debug for NativeDesktopPixels {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeDesktopPixels")
            .field("len", &self.bytes.as_deref().map_or(0, <[u8]>::len))
            .finish()
    }
}

impl Deref for NativeDesktopPixels {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.bytes.as_deref().unwrap_or_default()
    }
}

impl Drop for NativeDesktopPixels {
    fn drop(&mut self) {
        let Some(bytes) = self.bytes.take() else {
            return;
        };
        if let Ok(mut pool) = self.pool.lock()
            && pool.len() < Self::MAX_POOLED_BUFFERS
        {
            pool.push(bytes);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeDesktopCursor {
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
pub(crate) enum NativeDesktopError {
    #[cfg(not(windows))]
    #[error("native desktop capture is only supported on Windows")]
    UnsupportedPlatform,
    #[error("Windows desktop operation failed: {0}")]
    WindowsApi(&'static str),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NativeDesktopCommand {
    PointerMove {
        x: f64,
        y: f64,
    },
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
    ReleaseInput,
    Refresh,
}

#[derive(Debug)]
pub(crate) struct NativeInputState {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) buttons: u8,
    pub(crate) keys: BTreeSet<(u16, bool)>,
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

pub(crate) fn fit_native_desktop_geometry(
    source: NativeDesktopGeometry,
    max_width: u32,
    max_height: u32,
) -> NativeDesktopGeometry {
    if source.width <= max_width && source.height <= max_height {
        return NativeDesktopGeometry {
            width: source.width.max(2) & !1,
            height: source.height.max(2) & !1,
            ..source
        };
    }

    let (width, height) = if u64::from(source.width) * u64::from(max_height)
        > u64::from(source.height) * u64::from(max_width)
    {
        (
            max_width,
            (u64::from(source.height) * u64::from(max_width) / u64::from(source.width)) as u32,
        )
    } else {
        (
            (u64::from(source.width) * u64::from(max_height) / u64::from(source.height)) as u32,
            max_height,
        )
    };

    NativeDesktopGeometry {
        origin_x: source.origin_x,
        origin_y: source.origin_y,
        width: width.max(2) & !1,
        height: height.max(2) & !1,
    }
}

pub(crate) fn scale_native_desktop_screens(
    screens: Vec<DesktopScreenResponse>,
    source: NativeDesktopGeometry,
    output: NativeDesktopGeometry,
) -> Vec<DesktopScreenResponse> {
    fn scale(value: u32, source: u32, output: u32) -> u32 {
        ((u64::from(value) * u64::from(output) + u64::from(source) / 2) / u64::from(source)) as u32
    }

    screens
        .into_iter()
        .map(|screen| {
            let x = scale(screen.x, source.width, output.width).min(output.width);
            let y = scale(screen.y, source.height, output.height).min(output.height);
            let right = scale(
                screen.x.saturating_add(screen.width),
                source.width,
                output.width,
            )
            .min(output.width);
            let bottom = scale(
                screen.y.saturating_add(screen.height),
                source.height,
                output.height,
            )
            .min(output.height);
            DesktopScreenResponse {
                x,
                y,
                width: right.saturating_sub(x).max(1),
                height: bottom.saturating_sub(y).max(1),
                ..screen
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::desktop::DesktopScreenResponse;

    use super::{
        NativeDesktopCommand, NativeDesktopGeometry, NativeDesktopPixels,
        fit_native_desktop_geometry, scale_native_desktop_screens,
    };

    #[test]
    fn accepts_release_keys_command() {
        let command =
            serde_json::from_str::<NativeDesktopCommand>(r#"{"type":"release_keys"}"#).unwrap();

        assert!(matches!(command, NativeDesktopCommand::ReleaseKeys));
    }

    #[test]
    fn accepts_release_input_command() {
        let command =
            serde_json::from_str::<NativeDesktopCommand>(r#"{"type":"release_input"}"#).unwrap();

        assert!(matches!(command, NativeDesktopCommand::ReleaseInput));
    }

    #[test]
    fn accepts_pointer_move_command() {
        let command = serde_json::from_str::<NativeDesktopCommand>(
            r#"{"type":"pointer_move","x":0.25,"y":0.75}"#,
        )
        .unwrap();

        assert!(matches!(
            command,
            NativeDesktopCommand::PointerMove { x, y }
                if x == 0.25 && y == 0.75
        ));
    }

    #[test]
    fn returns_captured_pixel_buffers_to_the_pool() {
        let pool = Arc::new(Mutex::new(Vec::new()));
        let pixels = NativeDesktopPixels::new(vec![1, 2, 3, 4], Arc::clone(&pool));

        assert_eq!(&*pixels, &[1, 2, 3, 4]);
        drop(pixels);

        assert_eq!(pool.lock().unwrap().pop(), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn fits_wide_virtual_desktops_inside_the_stream_cap() {
        let source = NativeDesktopGeometry {
            origin_x: -3_840,
            origin_y: 0,
            width: 7_680,
            height: 2_160,
        };

        assert_eq!(
            fit_native_desktop_geometry(source, 1_920, 1_080),
            NativeDesktopGeometry {
                origin_x: -3_840,
                origin_y: 0,
                width: 1_920,
                height: 540,
            }
        );
    }

    #[test]
    fn makes_uncapped_desktop_dimensions_safe_for_yuv420() {
        let source = NativeDesktopGeometry {
            origin_x: 0,
            origin_y: 0,
            width: 1_919,
            height: 1_079,
        };

        assert_eq!(
            fit_native_desktop_geometry(source, 1_920, 1_080),
            NativeDesktopGeometry {
                width: 1_918,
                height: 1_078,
                ..source
            }
        );
    }

    #[test]
    fn scales_monitor_layout_to_match_the_encoded_frame() {
        let source = NativeDesktopGeometry {
            origin_x: -1_920,
            origin_y: 0,
            width: 3_840,
            height: 1_080,
        };
        let output = fit_native_desktop_geometry(source, 1_920, 1_080);
        let screens = vec![
            DesktopScreenResponse {
                id: "display-2".to_string(),
                label: "2".to_string(),
                title: "Screen 2".to_string(),
                x: 0,
                y: 0,
                width: 1_920,
                height: 1_080,
                primary: false,
            },
            DesktopScreenResponse {
                id: "display-1".to_string(),
                label: "1".to_string(),
                title: "Screen 1".to_string(),
                x: 1_920,
                y: 0,
                width: 1_920,
                height: 1_080,
                primary: true,
            },
        ];

        let scaled = scale_native_desktop_screens(screens, source, output);

        assert_eq!((scaled[0].x, scaled[0].width), (0, 960));
        assert_eq!((scaled[1].x, scaled[1].width), (960, 960));
        assert_eq!(scaled[0].height, 540);
        assert_eq!(scaled[1].height, 540);
    }
}
