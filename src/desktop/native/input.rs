use super::{NativeDesktopCommand, NativeDesktopError, NativeInputState};

const MAX_TEXT_UNITS: usize = 4096;

#[cfg(windows)]
pub(crate) fn apply_native_desktop_command(
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
        NativeDesktopCommand::PointerMove { x, y } => {
            let x = x.clamp(0.0, 1.0);
            let y = y.clamp(0.0, 1.0);
            send(&[mouse_input(
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                (x * 65_535.0).round() as i32,
                (y * 65_535.0).round() as i32,
                0,
            )])?;
            state.x = x;
            state.y = y;
        }
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
        NativeDesktopCommand::ReleaseInput => {
            let mut first_error = None;
            if state.buttons != 0 {
                let command = NativeDesktopCommand::Pointer {
                    x: state.x,
                    y: state.y,
                    buttons: 0,
                };
                if let Err(error) = apply_native_desktop_command(command, state) {
                    first_error = Some(error);
                }
            }
            if !state.keys.is_empty()
                && let Err(error) =
                    apply_native_desktop_command(NativeDesktopCommand::ReleaseKeys, state)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
            if let Some(error) = first_error {
                return Err(error);
            }
        }
        NativeDesktopCommand::Text { text } => {
            let mut inputs = Vec::new();
            for unit in text.encode_utf16().take(MAX_TEXT_UNITS) {
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
pub(crate) fn apply_native_desktop_command(
    _command: NativeDesktopCommand,
    _state: &mut NativeInputState,
) -> Result<(), NativeDesktopError> {
    Err(NativeDesktopError::UnsupportedPlatform)
}
