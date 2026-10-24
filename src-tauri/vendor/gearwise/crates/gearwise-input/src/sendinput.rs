//! Windows sink: `SendInput`, with hardware scancodes for keys so games that
//! read raw input or DirectInput see them like real key presses.

use std::{io, mem};

use gearwise_core::model::MouseButton;
use gearwise_core::{Error, Result};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
    SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{WHEEL_DELTA, XBUTTON1, XBUTTON2};

use crate::winkeys::{self, WinKey};
use crate::{InputSink, wheel_ticks};

/// Stateless: every call goes straight to `SendInput`.
pub(crate) struct SendInputSink;

fn send(inputs: &[INPUT]) -> Result<()> {
    // SAFETY: `inputs` is a valid slice of fully initialised `INPUT`s and the
    // size argument is `size_of::<INPUT>()`, as `SendInput` requires.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        // 0 usually means UIPI blocked us (the focused window is elevated).
        Err(Error::transport(format!(
            "SendInput accepted {sent} of {} events: {}",
            inputs.len(),
            io::Error::last_os_error()
        )))
    }
}

fn keyboard(vk: u16, scan: u16, flags: u32) -> INPUT {
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

fn mouse(data: u32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_input(usage: u8, down: bool) -> Result<INPUT> {
    let up = if down { 0 } else { KEYEVENTF_KEYUP };
    match winkeys::key(usage).ok_or(Error::Unsupported("key has no Windows scancode"))? {
        WinKey::Scan { code, extended } => {
            let ext = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
            Ok(keyboard(0, code, KEYEVENTF_SCANCODE | ext | up))
        }
        WinKey::Vk(vk) => Ok(keyboard(vk, 0, up)),
    }
}

fn button_input(button: MouseButton, down: bool) -> Option<INPUT> {
    let (down_flag, up_flag, data) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
        MouseButton::Back => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, u32::from(XBUTTON1)),
        MouseButton::Forward => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, u32::from(XBUTTON2)),
        MouseButton::WheelLeft | MouseButton::WheelRight => return None,
    };
    Some(mouse(data, if down { down_flag } else { up_flag }))
}

impl InputSink for SendInputSink {
    fn key_down(&mut self, usage: u8) -> Result<()> {
        send(&[key_input(usage, true)?])
    }

    fn key_up(&mut self, usage: u8) -> Result<()> {
        send(&[key_input(usage, false)?])
    }

    fn mouse_down(&mut self, button: MouseButton) -> Result<()> {
        match wheel_ticks(button) {
            Some((dx, dy)) => self.scroll(dx, dy),
            None => button_input(button, true).map_or(Ok(()), |i| send(&[i])),
        }
    }

    fn mouse_up(&mut self, button: MouseButton) -> Result<()> {
        button_input(button, false).map_or(Ok(()), |i| send(&[i]))
    }

    fn scroll(&mut self, dx: i32, dy: i32) -> Result<()> {
        // The wheel amount is a signed multiple of WHEEL_DELTA carried in the
        // unsigned `mouseData` field.
        let amount = |notches: i32| notches.saturating_mul(WHEEL_DELTA as i32) as u32;
        let mut inputs = Vec::with_capacity(2);
        if dx != 0 {
            inputs.push(mouse(amount(dx), MOUSEEVENTF_HWHEEL));
        }
        if dy != 0 {
            inputs.push(mouse(amount(dy), MOUSEEVENTF_WHEEL));
        }
        if inputs.is_empty() {
            Ok(())
        } else {
            send(&inputs)
        }
    }

    fn media(&mut self, consumer_usage: u16) -> Result<()> {
        let vk = winkeys::media(consumer_usage)
            .ok_or(Error::Unsupported("media key has no Windows virtual key"))?;
        send(&[keyboard(vk, 0, 0), keyboard(vk, 0, KEYEVENTF_KEYUP)])
    }
}
