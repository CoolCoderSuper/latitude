use super::{NativeDesktopCursor, NativeDesktopError, NativeDesktopFrame, NativeDesktopGeometry};

#[cfg(windows)]
pub(crate) struct NativeDesktopCapture {
    geometry: NativeDesktopGeometry,
    screen_dc: windows_sys::Win32::Graphics::Gdi::HDC,
    memory_dc: windows_sys::Win32::Graphics::Gdi::HDC,
    bitmap: windows_sys::Win32::Graphics::Gdi::HBITMAP,
    previous: windows_sys::Win32::Graphics::Gdi::HGDIOBJ,
    bits: *mut core::ffi::c_void,
    byte_len: usize,
}

#[cfg(not(windows))]
pub(crate) struct NativeDesktopCapture;

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
impl NativeDesktopCapture {
    pub(crate) fn new() -> Result<Self, NativeDesktopError> {
        use std::{mem::size_of, ptr::null_mut};

        use windows_sys::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, GetDC, HGDIOBJ, RGBQUAD, ReleaseDC, SelectObject,
        };

        let geometry = native_desktop_geometry()?;
        let width = geometry.width as i32;
        let height = geometry.height as i32;
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

        Ok(Self {
            geometry,
            screen_dc,
            memory_dc,
            bitmap,
            previous,
            bits,
            byte_len: (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4),
        })
    }

    pub(crate) fn capture(&mut self) -> Result<NativeDesktopFrame, NativeDesktopError> {
        use std::slice;

        use windows_sys::Win32::Graphics::Gdi::{BitBlt, CAPTUREBLT, SRCCOPY};

        let geometry = native_desktop_geometry()?;
        if geometry != self.geometry {
            *self = Self::new()?;
        }
        let copied = unsafe {
            BitBlt(
                self.memory_dc,
                0,
                0,
                self.geometry.width as i32,
                self.geometry.height as i32,
                self.screen_dc,
                self.geometry.origin_x,
                self.geometry.origin_y,
                SRCCOPY | CAPTUREBLT,
            )
        };
        if copied == 0 {
            return Err(NativeDesktopError::WindowsApi(
                "desktop pixels could not be copied",
            ));
        }

        let mut bgra = Vec::with_capacity(self.byte_len);
        bgra.extend_from_slice(unsafe {
            slice::from_raw_parts(self.bits.cast::<u8>(), self.byte_len)
        });
        Ok(NativeDesktopFrame {
            geometry: self.geometry,
            cursor: native_cursor_style(),
            bgra,
        })
    }
}

#[cfg(windows)]
impl Drop for NativeDesktopCapture {
    fn drop(&mut self) {
        use std::ptr::null_mut;

        use windows_sys::Win32::Graphics::Gdi::{
            DeleteDC, DeleteObject, HGDIOBJ, ReleaseDC, SelectObject,
        };

        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.memory_dc, self.previous);
            }
            DeleteObject(self.bitmap as HGDIOBJ);
            DeleteDC(self.memory_dc);
            ReleaseDC(null_mut(), self.screen_dc);
        }
    }
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
impl NativeDesktopCapture {
    pub(crate) fn new() -> Result<Self, NativeDesktopError> {
        Err(NativeDesktopError::UnsupportedPlatform)
    }

    pub(crate) fn capture(&mut self) -> Result<NativeDesktopFrame, NativeDesktopError> {
        Err(NativeDesktopError::UnsupportedPlatform)
    }
}
