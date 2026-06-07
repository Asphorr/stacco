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

    /// Builds a single mouse event carrying `flags`.
    fn mouse_event(flags: MOUSE_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        }
    }

    /// Injects `events` as one batch. A single `SendInput` call is atomic with
    /// respect to other input sources, so nothing can slip in between (e.g.)
    /// a button-press and its release.
    fn send_all(events: &[INPUT]) -> Result<()> {
        // SAFETY: every element is a fully-initialized mouse event and `cbSize`
        // is the size of one `INPUT`, exactly as `SendInput` requires.
        let sent = unsafe { SendInput(events, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize != events.len() {
            return Err(Error::Input(format!(
                "SendInput delivered {sent} of {} events (possibly blocked by UIPI)",
                events.len()
            )));
        }
        Ok(())
    }
}

impl InputBackend for WindowsBackend {
    fn click(&self, button: MouseButton) -> Result<()> {
        let (down, up) = Self::button_flags(button);
        // Press and release in one call so the pair is delivered atomically.
        Self::send_all(&[Self::mouse_event(down), Self::mouse_event(up)])
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
