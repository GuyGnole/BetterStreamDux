//! Reads `config.toml`, the file that says which key does what.

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Screen brightness, 0-100.
    #[serde(default = "default_brightness")]
    pub brightness: u8,

    /// How long a key must be held to count as a long press.
    #[serde(default = "default_long_press_ms")]
    pub long_press_ms: u64,

    #[serde(default, rename = "button")]
    pub buttons: Vec<Button>,
}

/// One key binding. Either `short_press` and/or `long_press`, or `while_press` alone.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    /// Key index, 0-14 on a Stream Deck MK.2.
    pub key: u8,
    /// Module name, e.g. "spotify".
    pub module: String,
    /// Action when the key is released before `long_press_ms`.
    pub short_press: Option<String>,
    /// Action when the key has been held for `long_press_ms`.
    pub long_press: Option<String>,
    /// Actions when the key goes down and when it comes back up (push-to-talk).
    pub while_press: Option<WhilePress>,
    /// Background color as "#RRGGBB".
    pub color: Option<String>,
    /// Path to a png/jpg image. Takes priority over `color`.
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhilePress {
    pub press: String,
    pub release: String,
}

fn default_brightness() -> u8 {
    60
}

fn default_long_press_ms() -> u64 {
    500
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
        Self::parse(&text).with_context(|| format!("invalid config in {path}"))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text)?;
        config.validate()?;
        Ok(config)
    }

    /// Returns the button bound to `key`, if any.
    pub fn button(&self, key: u8) -> Option<&Button> {
        self.buttons.iter().find(|b| b.key == key)
    }

    /// Catches binding mistakes at startup instead of when a key is pressed.
    fn validate(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for b in &self.buttons {
            let key = b.key;
            if !seen.insert(key) {
                bail!("key {key} is bound twice");
            }
            let has_press = b.short_press.is_some() || b.long_press.is_some();
            if b.while_press.is_some() && has_press {
                bail!("key {key}: while_press cannot be combined with short_press/long_press");
            }
            if b.while_press.is_none() && !has_press {
                bail!("key {key}: needs short_press, long_press or while_press");
            }
        }
        Ok(())
    }
}

/// Turns "#RRGGBB" into `[r, g, b]`.
pub fn parse_color(hex: &str) -> Result<[u8; 3]> {
    let hex = hex.trim_start_matches('#');
    anyhow::ensure!(hex.len() == 6, "color must look like #RRGGBB, got {hex}");
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_buttons() {
        let config = Config::parse(
            r#"
            brightness = 30
            [[button]]
            key = 1
            module = "spotify"
            short_press = "play_pause"
            long_press = "next"
            [[button]]
            key = 2
            module = "discord"
            while_press = { press = "unmute", release = "mute" }
            "#,
        )
        .unwrap();
        assert_eq!(config.brightness, 30);
        assert_eq!(config.long_press_ms, 500);
        assert_eq!(
            config.button(1).unwrap().long_press.as_deref(),
            Some("next")
        );
        assert_eq!(
            config
                .button(2)
                .unwrap()
                .while_press
                .as_ref()
                .unwrap()
                .release,
            "mute"
        );
        assert!(config.button(3).is_none());
    }

    #[test]
    fn rejects_while_press_mixed_with_short_press() {
        let err = Config::parse(
            r#"
            [[button]]
            key = 1
            module = "x"
            short_press = "a"
            while_press = { press = "b", release = "c" }
            "#,
        );
        assert!(err.is_err());
    }

    #[test]
    fn rejects_button_without_action() {
        assert!(Config::parse("[[button]]\nkey = 1\nmodule = \"x\"").is_err());
    }

    #[test]
    fn rejects_duplicate_keys() {
        let text = "[[button]]\nkey = 1\nmodule = \"x\"\nshort_press = \"a\"\n";
        assert!(Config::parse(&text.repeat(2)).is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        // Catches typos and the old `action = ...` format.
        assert!(Config::parse("[[button]]\nkey = 1\nmodule = \"x\"\naction = \"a\"").is_err());
    }

    #[test]
    fn example_config_is_valid() {
        Config::load("config.toml").unwrap();
    }

    #[test]
    fn parses_colors() {
        assert_eq!(parse_color("#1DB954").unwrap(), [0x1D, 0xB9, 0x54]);
        assert!(parse_color("#123").is_err());
    }
}
