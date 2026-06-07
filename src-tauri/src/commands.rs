//! Tauri command handlers — the IPC surface the webview calls via `invoke`.
//!
//! Each command is a thin adapter: validate, delegate to the engine/state, and
//! return a `Result` whose error serializes to a string for the frontend.

use tauri::{AppHandle, Manager, State};

use crate::config::{ClickConfig, CloseBehavior};
use crate::engine::Status;
use crate::error::Result;
use crate::input::Point;
use crate::state::AppState;
use crate::{hotkey, persistence};

/// Returns the configuration the UI should render on startup.
#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> ClickConfig {
    state.config()
}

/// Stores a new configuration without starting (keeps the hotkey-start path in
/// sync with the latest UI settings).
#[tauri::command]
pub fn set_config(config: ClickConfig, state: State<'_, AppState>) -> Result<()> {
    config.validate()?;
    state.set_config(config);
    Ok(())
}

/// Starts (or restarts) clicking with `config`.
#[tauri::command]
pub fn start(config: ClickConfig, state: State<'_, AppState>) -> Result<()> {
    config.validate()?;
    state.set_config(config.clone());
    state.engine.start(config)
}

/// Stops clicking.
#[tauri::command]
pub fn stop(state: State<'_, AppState>) -> Result<()> {
    state.engine.stop()
}

/// Returns `{ running, clicks }` for the UI to poll.
#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Status {
    state.engine.status()
}

/// Reads the current cursor position for the "capture point" button.
#[tauri::command]
pub fn get_cursor_position(state: State<'_, AppState>) -> Result<Point> {
    state.engine.cursor_position()
}

/// Validates and registers a new global toggle hotkey, then stores it.
#[tauri::command]
pub fn set_hotkey(app: AppHandle, hotkey: String, state: State<'_, AppState>) -> Result<()> {
    hotkey::set_toggle_hotkey(&app, &hotkey)?;
    let mut config = state.config();
    config.hotkey = hotkey;
    config.validate()?;
    state.set_config(config);
    Ok(())
}

/// Persists the current configuration to disk.
#[tauri::command]
pub fn save_config(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    persistence::save(&app, &state.config())
}

/// Resolves the close dialog: optionally remembers the choice as the new close
/// behavior, then hides to the tray or quits.
#[tauri::command]
pub fn apply_close(
    app: AppHandle,
    state: State<'_, AppState>,
    quit: bool,
    remember: bool,
) -> Result<()> {
    if remember {
        let mut config = state.config();
        config.close_behavior = if quit {
            CloseBehavior::Quit
        } else {
            CloseBehavior::Tray
        };
        state.set_config(config.clone());
        persistence::save(&app, &config)?;
    }
    if quit {
        app.exit(0);
    } else if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    Ok(())
}
