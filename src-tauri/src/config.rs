//! Strongly-typed click configuration shared between the frontend and backend.
//!
//! Every option the UI exposes is modeled as an enum or bounded integer rather
//! than a loose string, so invalid states are unrepresentable and the compiler
//! enforces exhaustive handling. The types are `serde`-(de)serializable using
//! the exact shapes the JavaScript frontend sends.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Lower bound for the click interval. One millisecond is already faster than
/// any human and close to the scheduler's resolution.
pub const MIN_INTERVAL_MS: u64 = 1;

/// Upper bound for the click interval (one hour) — purely a sanity guard.
pub const MAX_INTERVAL_MS: u64 = 60 * 60 * 1000;

/// Which mouse button to actuate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

/// Whether each actuation is a single or a double click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClickType {
    #[default]
    Single,
    Double,
}

/// Where the click lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Position {
    /// Click wherever the cursor currently is.
    Current,
    /// Move the cursor to a fixed screen coordinate before each click.
    Fixed { x: i32, y: i32 },
}

impl Default for Position {
    fn default() -> Self {
        Self::Current
    }
}

/// How many times to click before stopping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Repeat {
    /// Keep clicking until the user stops it.
    UntilStopped,
    /// Click exactly `times` times, then stop automatically.
    Count { times: u64 },
}

impl Default for Repeat {
    fn default() -> Self {
        Self::UntilStopped
    }
}

/// Optional per-click randomization so the clicking looks less robotic.
///
/// All-zero means "no jitter", which is the default — the behavior is opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Jitter {
    /// Random ± variation of the interval, as a percentage of it (0–100).
    pub interval_pct: u8,
    /// Random ± variation of the click position in pixels (fixed point only).
    pub position_px: u32,
}

/// A complete, validated description of an autoclick session.
///
/// `#[serde(default)]` makes deserialization tolerant of missing fields, so an
/// older on-disk config keeps loading after new fields are added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClickConfig {
    /// Delay between clicks, in milliseconds.
    pub interval_ms: u64,
    /// Which button to click.
    pub button: MouseButton,
    /// Single or double click.
    pub click_type: ClickType,
    /// Cursor position strategy.
    pub position: Position,
    /// Stop condition.
    pub repeat: Repeat,
    /// Per-click randomization (off by default).
    pub jitter: Jitter,
    /// The global toggle hotkey, e.g. `"F6"` or `"CmdOrCtrl+Shift+K"`.
    pub hotkey: String,
}

impl Default for ClickConfig {
    fn default() -> Self {
        Self {
            interval_ms: 100,
            button: MouseButton::Left,
            click_type: ClickType::Single,
            position: Position::Current,
            repeat: Repeat::UntilStopped,
            jitter: Jitter::default(),
            hotkey: "F6".to_owned(),
        }
    }
}

impl ClickConfig {
    /// Returns `Ok(())` if every field is within its allowed range.
    ///
    /// # Errors
    /// Returns [`Error::InvalidConfig`] describing the first invalid field.
    pub fn validate(&self) -> Result<()> {
        if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&self.interval_ms) {
            return Err(Error::InvalidConfig(format!(
                "interval {} ms is outside the allowed range {MIN_INTERVAL_MS}..={MAX_INTERVAL_MS} ms",
                self.interval_ms
            )));
        }
        if let Repeat::Count { times } = self.repeat {
            if times == 0 {
                return Err(Error::InvalidConfig(
                    "click count must be at least 1".to_owned(),
                ));
            }
        }
        if self.jitter.interval_pct > 100 {
            return Err(Error::InvalidConfig(
                "interval jitter must be between 0 and 100 percent".to_owned(),
            ));
        }
        if self.hotkey.trim().is_empty() {
            return Err(Error::InvalidConfig("hotkey must not be empty".to_owned()));
        }
        Ok(())
    }

    /// The configured interval as a [`Duration`].
    #[must_use]
    pub fn interval(&self) -> Duration {
        Duration::from_millis(self.interval_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        assert!(ClickConfig::default().validate().is_ok());
    }

    #[test]
    fn rejects_zero_interval() {
        let cfg = ClickConfig {
            interval_ms: 0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_zero_count() {
        let cfg = ClickConfig {
            repeat: Repeat::Count { times: 0 },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn position_serializes_with_mode_tag() {
        let json = serde_json::to_string(&Position::Fixed { x: 10, y: 20 }).unwrap();
        assert_eq!(json, r#"{"mode":"fixed","x":10,"y":20}"#);
    }

    #[test]
    fn config_round_trips_through_json() {
        let cfg = ClickConfig {
            interval_ms: 250,
            button: MouseButton::Right,
            click_type: ClickType::Double,
            position: Position::Fixed { x: 1, y: 2 },
            repeat: Repeat::Count { times: 7 },
            jitter: Jitter {
                interval_pct: 20,
                position_px: 3,
            },
            hotkey: "CmdOrCtrl+K".to_owned(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ClickConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn rejects_excessive_jitter() {
        let cfg = ClickConfig {
            jitter: Jitter {
                interval_pct: 150,
                position_px: 0,
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // Only `intervalMs` provided; everything else should default.
        let cfg: ClickConfig = serde_json::from_str(r#"{"intervalMs": 42}"#).unwrap();
        assert_eq!(cfg.interval_ms, 42);
        assert_eq!(cfg.button, MouseButton::Left);
        assert_eq!(cfg.repeat, Repeat::UntilStopped);
    }
}
