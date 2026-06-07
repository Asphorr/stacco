//! Crate-wide error type.
//!
//! A single [`Error`] enum models every failure mode in the backend. It
//! implements [`serde::Serialize`] so that any `#[tauri::command]` can return
//! `Result<T, Error>` and have the error delivered to the frontend as a string.

use thiserror::Error;

/// All errors that can originate from the autoclicker backend.
#[derive(Debug, Error)]
pub enum Error {
    /// The supplied [`crate::config::ClickConfig`] failed validation.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// The OS rejected or dropped a synthesized input event.
    #[error("input synthesis failed: {0}")]
    Input(String),

    /// A command could not reach the background worker (it has shut down).
    #[error("the clicker worker is unavailable")]
    WorkerUnavailable,

    /// Registering or parsing a global hotkey failed.
    #[error("hotkey error: {0}")]
    Hotkey(String),

    /// A filesystem path could not be resolved.
    #[error("path error: {0}")]
    Path(String),

    /// An I/O error while reading or writing the config file.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// A (de)serialization error for the persisted config.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

// Deliver errors to the webview as a plain string rather than a tagged object.
impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
