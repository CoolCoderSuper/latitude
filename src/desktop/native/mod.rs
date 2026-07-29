mod capture;
mod controller;
mod input;

use std::{
    collections::BTreeSet,
    ops::Deref,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) use capture::{NativeDesktopCapture, native_desktop_geometry};
pub(crate) use controller::{
    NativeControllerLeaseState, NativeInputController, native_input_controller,
};
pub(crate) use input::apply_native_desktop_command;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct NativeDesktopGeometry {
    pub(crate) origin_x: i32,
    pub(crate) origin_y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug)]
pub(crate) struct NativeDesktopFrame {
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

    fn new(bytes: Vec<u8>, pool: Arc<Mutex<Vec<Vec<u8>>>>) -> Self {
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{NativeDesktopCommand, NativeDesktopPixels};

    #[test]
    fn accepts_release_keys_command() {
        let command =
            serde_json::from_str::<NativeDesktopCommand>(r#"{"type":"release_keys"}"#).unwrap();

        assert!(matches!(command, NativeDesktopCommand::ReleaseKeys));
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
}
