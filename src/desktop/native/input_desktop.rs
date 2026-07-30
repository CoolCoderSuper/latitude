#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::{
        StationsAndDesktops::{
            CloseDesktop, GetThreadDesktop, GetUserObjectInformationW, HDESK, OpenInputDesktop,
            SetThreadDesktop, UOI_NAME,
        },
        SystemServices::MAXIMUM_ALLOWED,
        Threading::GetCurrentThreadId,
    },
};

use super::NativeDesktopError;

/// Owns the active input-desktop handle while a dedicated capture thread is attached to it.
#[cfg(windows)]
pub(crate) struct InputDesktop {
    original: HDESK,
    current: HDESK,
    name: String,
}

#[cfg(not(windows))]
pub(crate) struct InputDesktop;

#[cfg(windows)]
impl InputDesktop {
    pub(crate) fn attach_current_thread() -> Result<Self, NativeDesktopError> {
        let original = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
        let current = open_input_desktop()?;
        let name = match desktop_name(current) {
            Ok(name) => name,
            Err(error) => {
                unsafe {
                    CloseDesktop(current);
                }
                return Err(error);
            }
        };
        if unsafe { SetThreadDesktop(current) } == 0 {
            unsafe {
                CloseDesktop(current);
            }
            return Err(NativeDesktopError::WindowsApi(
                "capture thread could not attach to the active input desktop",
            ));
        }
        Ok(Self {
            original,
            current,
            name,
        })
    }

    pub(crate) fn refresh(&mut self) -> Result<bool, NativeDesktopError> {
        let next = open_input_desktop()?;
        let next_name = match desktop_name(next) {
            Ok(name) => name,
            Err(error) => {
                unsafe {
                    CloseDesktop(next);
                }
                return Err(error);
            }
        };
        if next_name == self.name {
            unsafe {
                CloseDesktop(next);
            }
            return Ok(false);
        }
        if unsafe { SetThreadDesktop(next) } == 0 {
            unsafe {
                CloseDesktop(next);
            }
            return Err(NativeDesktopError::WindowsApi(
                "capture thread could not switch to the active input desktop",
            ));
        }

        let previous = std::mem::replace(&mut self.current, next);
        self.name = next_name;
        unsafe {
            CloseDesktop(previous);
        }
        Ok(true)
    }
}

#[cfg(windows)]
impl Drop for InputDesktop {
    fn drop(&mut self) {
        unsafe {
            if !self.original.is_null() {
                SetThreadDesktop(self.original);
            }
            CloseDesktop(self.current);
        }
    }
}

#[cfg(windows)]
fn open_input_desktop() -> Result<HDESK, NativeDesktopError> {
    let desktop = unsafe { OpenInputDesktop(0, 0, MAXIMUM_ALLOWED) };
    if desktop.is_null() {
        return Err(NativeDesktopError::WindowsApi(
            "active input desktop could not be opened",
        ));
    }
    Ok(desktop)
}

#[cfg(windows)]
fn desktop_name(desktop: HDESK) -> Result<String, NativeDesktopError> {
    let mut buffer = [0_u16; 256];
    let mut needed = 0;
    let read = unsafe {
        GetUserObjectInformationW(
            desktop as HANDLE,
            UOI_NAME,
            buffer.as_mut_ptr().cast(),
            std::mem::size_of_val(&buffer) as u32,
            &mut needed,
        )
    };
    if read == 0 {
        return Err(NativeDesktopError::WindowsApi(
            "active input desktop name could not be read",
        ));
    }
    let length = buffer
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..length]))
}

#[cfg(not(windows))]
impl InputDesktop {
    pub(crate) fn attach_current_thread() -> Result<Self, NativeDesktopError> {
        Err(NativeDesktopError::UnsupportedPlatform)
    }

    pub(crate) fn refresh(&mut self) -> Result<bool, NativeDesktopError> {
        Err(NativeDesktopError::UnsupportedPlatform)
    }
}
