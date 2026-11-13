//! Works out what each of the 15 keys should show right now.
//!
//! Pure function: live values come from the `values` lookup, no hardware or clock here.
//! `deck.rs` draws the result.

use crate::config::{Button, Config, KEY_COUNT, ValueRef, parse_color};
use crate::pages::View;

/// What one key looks like. Compared with the previous frame to redraw only changed keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
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

/// Looks up a live value such as "spotify:title". `None` when unavailable.
pub type Values<'a> = dyn Fn(&ValueRef) -> Option<String> + 'a;

pub fn build(config: &Config, view: &View, brightness: u8, values: &Values) -> Screen {
    let keys = (0..KEY_COUNT)
        .map(|key| {
            config
                .shown_button(view, key)
                .map_or_else(KeyFace::default, |button| face(button, values))
        })
        .collect();
    Screen { brightness, keys }
}

/// Live values win; the fixed fields are the fallback when a value is unavailable.
fn face(button: &Button, values: &Values) -> KeyFace {
    let text = button
        .display
        .as_ref()
        .and_then(values)
        .or_else(|| button.label.clone());
    let icon = button
        .icon_value
        .as_ref()
        .and_then(values)
        .or_else(|| button.icon.clone());
    let color = button
        .color_by
        .as_ref()
        .and_then(|c| c.colors.get(&values(&c.value)?))
        .or(button.color.as_ref())
        .and_then(|c| parse_color(c).ok());
    KeyFace { color, icon, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r##"
        [[button]]
        key = 0
        display = "deck:clock"
        [[button]]
        key = 1
        module = "deck"
        short_press = "open_folder f"
        label = "Folder"
        color = "#FF0000"
        [[button]]
        key = 2
        display = "spotify:title"
        label = "Spotify"
        icon = "default.png"
        icon_value = "spotify:cover"
        color = "#000000"
        color_by = { value = "spotify:status", colors = { playing = "#00FF00" } }
        [[button]]
        folder = "f"
        key = 0
        module = "deck"
        short_press = "back"
        display = "deck:page_number"
    "##;

    /// Fake values: the deck ones always, Spotify ones only if `spotify_open`.
    fn values(spotify_open: bool) -> impl Fn(&ValueRef) -> Option<String> {
        move |v| {
            let value = match (v.module.as_str(), v.name.as_str()) {
                ("deck", "clock") => "12:34",
                ("deck", "page_number") => "f",
                ("spotify", "title") if spotify_open => "Song",
                ("spotify", "cover") if spotify_open => "cover.jpg",
                ("spotify", "status") if spotify_open => "playing",
                _ => return None,
            };
            Some(value.to_string())
        }
    }

    #[test]
    fn shows_only_current_view_with_live_text() {
        let config = Config::parse(CONFIG).unwrap();

        let page = build(&config, &View::Page(1), 60, &values(true));
        assert_eq!(page.keys[0].text.as_deref(), Some("12:34"));
        assert_eq!(page.keys[1].text.as_deref(), Some("Folder"));
        assert_eq!(page.keys[1].color, Some([255, 0, 0]));
        assert_eq!(page.keys[3], KeyFace::default());

        let folder = build(&config, &View::Folder("f".into()), 60, &values(true));
        assert_eq!(folder.keys[0].text.as_deref(), Some("f"));
        assert_eq!(folder.keys[1], KeyFace::default());
    }

    #[test]
    fn module_values_with_fallbacks() {
        let config = Config::parse(CONFIG).unwrap();

        let open = build(&config, &View::Page(1), 60, &values(true));
        assert_eq!(
            open.keys[2],
            KeyFace {
                color: Some([0, 255, 0]),
                icon: Some("cover.jpg".into()),
                text: Some("Song".into())
            }
        );

        let closed = build(&config, &View::Page(1), 60, &values(false));
        assert_eq!(
            closed.keys[2],
            KeyFace {
                color: Some([0, 0, 0]),
                icon: Some("default.png".into()),
                text: Some("Spotify".into())
            }
        );
    }
}
