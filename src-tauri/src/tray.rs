//! System-tray menu construction and live (re)localization.
//!
//! The backend keeps only the built-in English labels (the source strings). The
//! full set of translations lives in the frontend catalog (`src/i18n.js`); the
//! frontend pushes the active locale's labels through [`set_labels`] so the
//! native tray matches the in-app language without the backend duplicating any
//! translation table — there is nothing here for the two to drift apart on.

use serde::Deserialize;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::{AppHandle, Runtime};

use crate::error::{Error, Result};

/// Stable id used to look the tray icon back up when relabeling it.
pub const TRAY_ID: &str = "main-tray";

/// Localized tray-menu labels, sent from the frontend.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLabels {
    pub show: String,
    pub toggle: String,
    pub quit: String,
    pub tooltip: String,
}

impl Default for TrayLabels {
    /// English source strings, shown until the frontend reports the user's
    /// locale. This is the fallback only — never a translation table.
    fn default() -> Self {
        Self {
            show: "Show Window".to_owned(),
            toggle: "Start / Stop".to_owned(),
            quit: "Quit".to_owned(),
            tooltip: "Stacco".to_owned(),
        }
    }
}

/// Builds the tray menu. The item ids (`show` / `toggle` / `quit`) are stable so
/// the tray's menu-event handler keeps matching across relabeling.
///
/// # Errors
/// Propagates any failure from the Tauri menu builder.
pub fn build_menu<R: Runtime>(app: &AppHandle<R>, labels: &TrayLabels) -> tauri::Result<Menu<R>> {
    let show_i = MenuItemBuilder::with_id("show", &labels.show).build(app)?;
    let toggle_i = MenuItemBuilder::with_id("toggle", &labels.toggle).build(app)?;
    let quit_i = MenuItemBuilder::with_id("quit", &labels.quit).build(app)?;
    MenuBuilder::new(app)
        .item(&show_i)
        .item(&toggle_i)
        .separator()
        .item(&quit_i)
        .build()
}

/// Re-labels the existing tray icon's menu and tooltip in place.
///
/// A missing tray (e.g. the icon could not be created at startup) is not an
/// error — there is simply nothing to localize.
///
/// # Errors
/// Returns [`Error::Tray`] if the menu cannot be rebuilt or swapped.
pub fn set_labels<R: Runtime>(app: &AppHandle<R>, labels: &TrayLabels) -> Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let menu = build_menu(app, labels).map_err(|e| Error::Tray(e.to_string()))?;
    tray.set_menu(Some(menu)).map_err(|e| Error::Tray(e.to_string()))?;
    tray.set_tooltip(Some(&labels.tooltip))
        .map_err(|e| Error::Tray(e.to_string()))?;
    Ok(())
}
