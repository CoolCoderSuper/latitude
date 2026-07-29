use serde::Serialize;
use thiserror::Error;

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawDesktopScreen {
    device: String,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    primary: bool,
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
