//! Reads `config.toml`, the file that says which key does what.

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Screen brightness, 0-100.
    #[serde(default = "default_brightness")]
    pub brightness: u8,

    #[serde(default, rename = "button")]
    pub buttons: Vec<Button>,
}

#[derive(Debug, Deserialize)]
pub struct Button {
    /// Key index, 0-14 on a Stream Deck MK.2.
    pub key: u8,
    /// Module name, e.g. "spotify".
    pub module: String,
    /// Action passed to the module, e.g. "`play_pause`".
    pub action: String,
    /// Background color as "#RRGGBB".
    pub color: Option<String>,
    /// Path to a png/jpg image. Takes priority over `color`.
    pub icon: Option<String>,
}

fn default_brightness() -> u8 {
    60
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
        Self::parse(&text).with_context(|| format!("invalid config in {path}"))
    }

    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// Returns the button bound to `key`, if any.
    pub fn button(&self, key: u8) -> Option<&Button> {
        self.buttons.iter().find(|b| b.key == key)
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
            action = "play_pause"
            "#,
        )
        .unwrap();
        assert_eq!(config.brightness, 30);
        assert_eq!(config.button(1).unwrap().module, "spotify");
        assert!(config.button(2).is_none());
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
