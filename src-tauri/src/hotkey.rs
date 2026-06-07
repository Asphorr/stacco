//! Global hotkey registration and the toggle handler.
//!
//! All hotkey logic lives on the Rust side, driven by the
//! `tauri-plugin-global-shortcut` plugin. Because registration happens in the
//! backend (not via IPC from the webview), it needs no ACL capability entry.

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::error::{Error, Result};
use crate::state::AppState;

/// Event emitted to the frontend whenever the hotkey toggles the engine, so the
/// UI updates instantly instead of waiting for the next status poll. The payload
/// is the new running state (`bool`).
pub const TOGGLE_EVENT: &str = "clicker:toggled";

/// Parses a human-readable accelerator such as `"F6"` or `"CmdOrCtrl+Shift+K"`.
///
/// # Errors
/// Returns [`Error::Hotkey`] if the string is not a valid accelerator.
pub fn parse_hotkey(hotkey: &str) -> Result<Shortcut> {
    hotkey
        .parse::<Shortcut>()
        .map_err(|e| Error::Hotkey(format!("invalid hotkey '{hotkey}': {e}")))
}

/// Registers `hotkey` as the global toggle, clearing any previous registration
/// first so changing the hotkey never leaks the old one.
///
/// # Errors
/// Returns [`Error::Hotkey`] if the accelerator is invalid or the OS refuses the
/// registration (e.g. another application already owns the combination).
pub fn set_toggle_hotkey<R: Runtime>(app: &AppHandle<R>, hotkey: &str) -> Result<()> {
    let shortcut = parse_hotkey(hotkey)?;
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();
    manager
        .register(shortcut)
        .map_err(|e| Error::Hotkey(e.to_string()))
}

/// Toggles the engine using the currently stored configuration and notifies the
/// frontend. Invoked from the plugin's key handler.
pub fn handle_toggle<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let config = state.config();
    match state.engine.toggle(config) {
        Ok(running) => {
            if let Err(e) = app.emit(TOGGLE_EVENT, running) {
                log::warn!("failed to emit toggle event: {e}");
            }
        }
        Err(e) => log::error!("hotkey toggle failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_bare_function_key() {
        assert!(parse_hotkey("F6").is_ok());
    }

    #[test]
    fn parses_a_modified_combination() {
        assert!(parse_hotkey("CmdOrCtrl+Shift+K").is_ok());
    }

    #[test]
    fn rejects_an_empty_accelerator() {
        assert!(parse_hotkey("").is_err());
    }
}
