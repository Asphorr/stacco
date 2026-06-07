//! Application state shared across Tauri commands and the hotkey handler.

use std::sync::{Arc, Mutex, PoisonError};

use crate::config::{ClickConfig, CloseBehavior};
use crate::engine::ClickerEngine;
use crate::error::Result;
use crate::input::{InputBackend, Point};

/// Managed by Tauri and retrieved in commands via `State<'_, AppState>`.
///
/// The engine is internally thread-safe, so it is exposed directly. The input
/// backend lives here — not inside the engine — so the UI can query the cursor
/// without routing through the click worker. The configuration is guarded by a
/// [`Mutex`] because both the UI (commands) and the global hotkey handler touch
/// it.
pub struct AppState {
    pub engine: ClickerEngine,
    backend: Arc<dyn InputBackend>,
    config: Mutex<ClickConfig>,
}

impl AppState {
    #[must_use]
    pub fn new(engine: ClickerEngine, backend: Arc<dyn InputBackend>, config: ClickConfig) -> Self {
        Self {
            engine,
            backend,
            config: Mutex::new(config),
        }
    }

    /// Returns a clone of the current configuration.
    ///
    /// A poisoned lock is recovered rather than propagated: the guarded value is
    /// only ever wholesale-replaced or cloned, so it is always left consistent —
    /// a panic in some unrelated thread must not take the whole app down.
    #[must_use]
    pub fn config(&self) -> ClickConfig {
        self.config
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replaces the stored configuration.
    pub fn set_config(&self, config: ClickConfig) {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = config;
    }

    /// The configured behavior for closing the window.
    #[must_use]
    pub fn close_behavior(&self) -> CloseBehavior {
        self.config
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .close_behavior
    }

    /// Reads the current cursor position (used by the "capture point" button).
    ///
    /// # Errors
    /// Propagates backend failures.
    pub fn cursor_position(&self) -> Result<Point> {
        self.backend.cursor_position()
    }
}
