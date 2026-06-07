//! Windows implementation of [`InputBackend`] using the Win32 `SendInput` API.

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEINPUT, MOUSE_EVENT_FLAGS,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

use super::{InputBackend, Point};
use crate::config::MouseButton;
use crate::error::{Error, Result};

/// Synthesizes input through `SendInput`. Zero-sized: it holds no state.
#[derive(Debug, Default)]
pub struct WindowsBackend;

impl WindowsBackend {
    /// Maps a logical button to its (button-down, button-up) event flags.
    fn button_flags(button: MouseButton) -> (MOUSE_EVENT_FLAGS, MOUSE_EVENT_FLAGS) {
        match button {
            MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        }
    }

    /// Sends a single mouse event carrying `flags`.
    fn send(flags: MOUSE_EVENT_FLAGS) -> Result<()> {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };

        // SAFETY: `input` is a fully-initialized mouse event; the slice length
        // matches the implicit count and `cbSize` is the size of one `INPUT`.
        let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
        if sent == 0 {
            return Err(Error::Input(
                "SendInput delivered no events (possibly blocked by UIPI)".to_owned(),
            ));
        }
        Ok(())
    }
}

impl InputBackend for WindowsBackend {
    fn click(&self, button: MouseButton) -> Result<()> {
        let (down, up) = Self::button_flags(button);
        Self::send(down)?;
        Self::send(up)
    }

    fn move_cursor(&self, point: Point) -> Result<()> {
        // SAFETY: plain FFI call with by-value integer arguments.
        unsafe { SetCursorPos(point.x, point.y) }
            .map_err(|e| Error::Input(format!("SetCursorPos failed: {e}")))
    }

    fn cursor_position(&self) -> Result<Point> {
        let mut pt = POINT::default();
        // SAFETY: `pt` is a valid, writable `POINT` for the duration of the call.
        unsafe { GetCursorPos(&mut pt) }
            .map_err(|e| Error::Input(format!("GetCursorPos failed: {e}")))?;
        Ok(Point { x: pt.x, y: pt.y })
    }
}
