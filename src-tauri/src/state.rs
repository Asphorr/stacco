//! Application state shared across Tauri commands and the hotkey handler.

use std::sync::Mutex;

use crate::config::{ClickConfig, CloseBehavior};
use crate::engine::ClickerEngine;

/// Managed by Tauri and retrieved in commands via `State<'_, AppState>`.
///
/// The engine is internally thread-safe, so it is exposed directly. The current
/// configuration is guarded by a [`Mutex`] because both the UI (commands) and
/// the global hotkey handler read and write it.
pub struct AppState {
    pub engine: ClickerEngine,
    config: Mutex<ClickConfig>,
}

impl AppState {
    #[must_use]
    pub fn new(engine: ClickerEngine, config: ClickConfig) -> Self {
        Self {
            engine,
            config: Mutex::new(config),
        }
    }

    /// Returns a clone of the current configuration.
    #[must_use]
    pub fn config(&self) -> ClickConfig {
        self.config
            .lock()
            .expect("config mutex poisoned")
            .clone()
    }

    /// Replaces the stored configuration.
    pub fn set_config(&self, config: ClickConfig) {
        *self.config.lock().expect("config mutex poisoned") = config;
    }

    /// The configured behavior for closing the window.
    #[must_use]
    pub fn close_behavior(&self) -> CloseBehavior {
        self.config
            .lock()
            .expect("config mutex poisoned")
            .close_behavior
    }
}
