use super::{NativeDesktopCursor, NativeDesktopError, NativeDesktopGeometry};

#[cfg(windows)]
pub(crate) fn native_desktop_geometry() -> Result<NativeDesktopGeometry, NativeDesktopError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    let origin_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let origin_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) } & !1;
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) } & !1;
    if width <= 0 || height <= 0 {
        return Err(NativeDesktopError::WindowsApi(
            "virtual desktop dimensions are unavailable",
        ));
    }

    Ok(NativeDesktopGeometry {
        origin_x,
        origin_y,
        width: width as u32,
        height: height as u32,
    })
}

#[cfg(not(windows))]
pub(crate) fn native_desktop_geometry() -> Result<NativeDesktopGeometry, NativeDesktopError> {
    Err(NativeDesktopError::UnsupportedPlatform)
}

#[cfg(windows)]
pub(crate) fn native_cursor_style() -> NativeDesktopCursor {
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
