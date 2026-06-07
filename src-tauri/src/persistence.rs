//! Loading and saving the configuration to the per-user app config directory.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};

use crate::config::ClickConfig;
use crate::error::{Error, Result};

/// File name inside the app config directory.
const CONFIG_FILE: &str = "config.json";

/// Resolves `<app config dir>/config.json` for the current user.
fn config_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| Error::Path(format!("cannot resolve app config dir: {e}")))?;
    Ok(dir.join(CONFIG_FILE))
}

/// Loads the saved config, falling back to defaults on any error (missing file,
/// corrupt JSON, failed validation). A first run is therefore not an error.
#[must_use]
pub fn load<R: Runtime>(app: &AppHandle<R>) -> ClickConfig {
    match try_load(app) {
        Ok(config) => config,
        Err(e) => {
            log::info!("falling back to default config: {e}");
            ClickConfig::default()
        }
    }
}

fn try_load<R: Runtime>(app: &AppHandle<R>) -> Result<ClickConfig> {
    let bytes = std::fs::read(config_path(app)?)?;
    let config: ClickConfig = serde_json::from_slice(&bytes)?;
    config.validate()?;
    Ok(config)
}

/// Persists `config` as pretty-printed JSON, creating the directory if needed.
///
/// # Errors
/// Returns an error if the path cannot be resolved or the file cannot be written.
pub fn save<R: Runtime>(app: &AppHandle<R>, config: &ClickConfig) -> Result<()> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(config)?;
    std::fs::write(&path, json)?;
    Ok(())
}
