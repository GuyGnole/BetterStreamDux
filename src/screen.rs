//! Works out what each of the 15 keys should show right now.
//!
//! Pure function: no hardware, no clock reading. `deck.rs` draws the result.

use crate::config::{Config, Display, KEY_COUNT, parse_color};
use crate::pages::View;

/// What one key looks like. Compared with the previous frame to redraw only changed keys.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyFace {
    pub color: Option<[u8; 3]>,
    pub icon: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub brightness: u8,
    pub keys: Vec<KeyFace>,
}

/// Values that `display = ...` keys can show.
pub struct LiveValues {
    pub clock: String,
    pub page_label: String,
    pub brightness: u8,
}

pub fn build(config: &Config, view: &View, live: &LiveValues) -> Screen {
    let mut keys = vec![KeyFace::default(); usize::from(KEY_COUNT)];
    for button in config.buttons.iter().filter(|b| b.view() == *view) {
        let text = match button.display {
            Some(Display::Clock) => Some(live.clock.clone()),
            Some(Display::PageNumber) => Some(live.page_label.clone()),
            Some(Display::Brightness) => Some(format!("{}%", live.brightness)),
            None => button.label.clone(),
        };
        keys[usize::from(button.key)] = KeyFace {
            color: button.color.as_deref().and_then(|c| parse_color(c).ok()),
            icon: button.icon.clone(),
            text,
        };
    }
    Screen {
        brightness: live.brightness,
        keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_only_current_view_with_live_text() {
        let config = Config::parse(
            r##"
            [[button]]
            key = 0
            display = "clock"
            [[button]]
            key = 1
            module = "deck"
            short_press = "open_folder f"
            label = "Folder"
            color = "#FF0000"
            [[button]]
            folder = "f"
            key = 0
            module = "deck"
            short_press = "back"
            display = "page_number"
            "##,
        )
        .unwrap();
        let live = LiveValues {
            clock: "12:34".into(),
            page_label: "f".into(),
            brightness: 60,
        };

        let page = build(&config, &View::Page(1), &live);
        assert_eq!(page.keys[0].text.as_deref(), Some("12:34"));
        assert_eq!(page.keys[1].text.as_deref(), Some("Folder"));
        assert_eq!(page.keys[1].color, Some([255, 0, 0]));
        assert_eq!(page.keys[2], KeyFace::default());

        let folder = build(&config, &View::Folder("f".into()), &live);
        assert_eq!(folder.keys[0].text.as_deref(), Some("f"));
        assert_eq!(folder.keys[1], KeyFace::default());
    }
}
