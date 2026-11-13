//! Copy, cut and paste of keys.
//!
//! A copied key goes to the system clipboard as a `[[button]]` block in config format,
//! so it can also be pasted into the config file (or copied from it).

use anyhow::{Context, Result};
use eframe::egui;
use serde::{Deserialize, Serialize};

use super::{Editor, edit};
use crate::config::Button;
use crate::pages::View;

/// The clipboard text: a list with one `[[button]]`, like in the config file.
#[derive(Serialize, Deserialize)]
struct Clip {
    button: Vec<Button>,
}

pub fn to_text(button: &Button) -> Result<String> {
    let mut button = button.clone();
    button.place(&View::Page(1), 0); // the position is chosen when pasting
    Ok(toml::to_string(&Clip {
        button: vec![button],
    })?)
}

/// A key from clipboard text: a `[[button]]` block, or just its fields.
pub fn from_text(text: &str) -> Result<Button> {
    let not_a_key = "the clipboard does not contain a key";
    if let Ok(clip) = toml::from_str::<Clip>(text) {
        return clip.button.into_iter().next().context(not_a_key);
    }
    toml::from_str::<Button>(text).context(not_a_key)
}

impl Editor {
    /// The key shown at `key` (its own, or the one from "All pages").
    fn shown(&self, key: u8) -> Option<&Button> {
        self.config.shown_button(&self.view, key)
    }

    pub fn copy(&mut self, ctx: &egui::Context, key: u8) {
        let Some(button) = self.shown(key).cloned() else {
            self.message = "Nothing to copy: the key is empty.".into();
            return;
        };
        match to_text(&button) {
            Ok(text) => {
                ctx.copy_text(text);
                self.clipboard = Some(button);
                self.message = format!("Copied key {key}.");
            }
            Err(err) => self.message = format!("Copy failed: {err:#}"),
        }
    }

    pub fn cut(&mut self, ctx: &egui::Context, key: u8) {
        if self.config.shows_shared(&self.view, key) {
            self.message = "This key comes from \"All pages\": cut it there.".into();
            return;
        }
        self.copy(ctx, key);
        if self.clipboard.is_some() {
            self.clear_key(key);
            self.message = format!("Cut key {key}.");
        }
    }

    /// Pastes `button` on `key`, replacing what was there.
    pub fn paste(&mut self, key: u8, button: Button) {
        edit::put(&mut self.config, &self.view, key, button);
        self.message = format!("Pasted on key {key}.");
    }

    /// Pastes clipboard text (from Ctrl+V), if it is a key.
    pub fn paste_text(&mut self, key: u8, text: &str) {
        match from_text(text) {
            Ok(button) => self.paste(key, button),
            Err(err) => self.message = format!("{err:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_round_trips_through_text() {
        let button = Button {
            symbol: Some("play".into()),
            short_press: vec!["spotify:play_pause".into(), "deck:next_page".into()],
            ..Button::new(&View::Folder("f".into()), 7)
        };
        let text = to_text(&button).unwrap();
        assert!(text.starts_with("[[button]]"), "config format:\n{text}");
        let back = from_text(&text).unwrap();
        assert_eq!(back.symbol, button.symbol);
        assert_eq!(back.short_press, button.short_press);
    }

    #[test]
    fn accepts_fields_alone_and_rejects_other_text() {
        let button = from_text("key = 0\nsymbol = \"mic\"").unwrap();
        assert_eq!(button.symbol.as_deref(), Some("mic"));
        assert!(from_text("hello world").is_err());
        assert!(from_text("").is_err());
    }
}
