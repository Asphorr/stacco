//! Platform input abstraction.
//!
//! The engine talks to the operating system exclusively through the
//! [`InputBackend`] trait. This keeps the click loop platform-agnostic and lets
//! tests substitute a fake backend instead of moving the real cursor.

use std::sync::Arc;

use crate::config::MouseButton;
use crate::error::Result;

/// A screen coordinate in physical pixels, origin at the top-left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Synthesizes mouse input and queries the cursor.
///
/// Implementors must be `Send + Sync`: the engine shares one backend between
/// the worker thread and the UI thread (for cursor queries) via an [`Arc`].
pub trait InputBackend: Send + Sync {
    /// Presses and releases `button` once at the current cursor location.
    ///
    /// # Errors
    /// Returns an error if the OS rejects the synthesized event.
    fn click(&self, button: MouseButton) -> Result<()>;

    /// Moves the cursor to `point`.
    ///
    /// # Errors
    /// Returns an error if the OS rejects the move.
    fn move_cursor(&self, point: Point) -> Result<()>;

    /// Returns the current cursor position.
    ///
    /// # Errors
    /// Returns an error if the position cannot be read.
    fn cursor_position(&self) -> Result<Point>;
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::WindowsBackend;

/// Builds the input backend for the current platform.
///
/// # Errors
/// Returns an error if the backend cannot be initialized.
#[cfg(windows)]
pub fn platform_backend() -> Result<Arc<dyn InputBackend>> {
    Ok(Arc::new(WindowsBackend))
}

/// Fallback no-op backend so the crate still compiles and tests run on
/// non-Windows hosts (CI, `cargo test` on Linux/macOS).
#[cfg(not(windows))]
#[derive(Debug, Default)]
pub struct NullBackend;

#[cfg(not(windows))]
impl InputBackend for NullBackend {
    fn click(&self, _button: MouseButton) -> Result<()> {
        Ok(())
    }
    fn move_cursor(&self, _point: Point) -> Result<()> {
        Ok(())
    }
    fn cursor_position(&self) -> Result<Point> {
        Ok(Point { x: 0, y: 0 })
    }
}

#[cfg(not(windows))]
pub fn platform_backend() -> Result<Arc<dyn InputBackend>> {
    Ok(Arc::new(NullBackend))
}
