//! Stacco — Tauri backend.
//!
//! Module map:
//! * [`config`]      — strongly-typed, validated click settings.
//! * [`input`]       — `InputBackend` trait + Win32 `SendInput` implementation.
//! * [`engine`]      — the worker thread that actually clicks.
//! * [`state`]       — shared application state managed by Tauri.
//! * [`hotkey`]      — global toggle hotkey registration and handling.
//! * [`persistence`] — load/save the config to the user's config dir.
//! * [`commands`]    — the `invoke` IPC surface exposed to the webview.
//! * [`error`]       — the crate-wide error type.

#![warn(clippy::all)]

mod commands;
mod config;
mod engine;
mod error;
mod hotkey;
mod input;
mod persistence;
mod state;
mod tray;

use tauri::tray::{MouseButton as TrayButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::config::CloseBehavior;
use crate::engine::ClickerEngine;
use crate::state::AppState;

/// Builds and runs the Tauri application.
///
/// # Panics
/// Panics only if the Tauri runtime itself fails to start, which is an
/// unrecoverable initialization error.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // React on key-down only; ignore the matching key-up.
                    if event.state() == ShortcutState::Pressed {
                        hotkey::handle_toggle(app);
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            // Closing follows the user's saved preference. Until they pick one,
            // a dialog asks whether to hide to the tray or quit — so nobody is
            // surprised that clicking keeps going after "closing".
            if let WindowEvent::CloseRequested { api, .. } = event {
                let behavior = window
                    .try_state::<AppState>()
                    .map_or(CloseBehavior::Tray, |s| s.close_behavior());
                match behavior {
                    CloseBehavior::Quit => window.app_handle().exit(0),
                    CloseBehavior::Tray => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    CloseBehavior::Ask => {
                        api.prevent_close();
                        let _ = window.emit("clicker:close-requested", ());
                    }
                }
            }
        })
        .setup(|app| {
            // Verbose logging in debug builds only.
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Wire up the input backend, engine, and persisted configuration.
            let backend = input::platform_backend()?;
            let engine = ClickerEngine::new(std::sync::Arc::clone(&backend));
            let config = persistence::load(app.handle());
            app.manage(AppState::new(engine, backend, config.clone()));

            // Register the global toggle hotkey. A failure here is non-fatal:
            // the app still works through the on-screen Start/Stop button.
            if let Err(e) = hotkey::set_toggle_hotkey(app.handle(), &config.hotkey) {
                log::warn!("could not register hotkey '{}': {e}", config.hotkey);
            }

            // System tray: keeps the app available while the window is hidden.
            // Left-click shows the window; the menu offers Show / Start-Stop / Quit.
            let handle = app.handle().clone();
            if let Some(icon) = handle.default_window_icon().cloned() {
                // Start with the built-in English labels; the frontend pushes
                // the user's locale via `set_tray_labels` as soon as it loads.
                let menu = tray::build_menu(&handle, &tray::TrayLabels::default())?;

                TrayIconBuilder::with_id(tray::TRAY_ID)
                    .icon(icon)
                    .tooltip("Stacco")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "show" => show_main_window(app),
                        "toggle" => hotkey::handle_toggle(app),
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray_icon, event| {
                        if let TrayIconEvent::Click {
                            button: TrayButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            show_main_window(tray_icon.app_handle());
                        }
                    })
                    .build(&handle)?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::start,
            commands::stop,
            commands::get_status,
            commands::get_cursor_position,
            commands::set_hotkey,
            commands::save_config,
            commands::apply_close,
            commands::set_tray_labels,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Brings the main window back to the foreground (from the tray or minimized).
fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
