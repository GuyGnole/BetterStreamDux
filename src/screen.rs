//! Works out what each of the 15 keys should show right now.
//!
//! Pure function: live values come from the `values` lookup, no hardware or clock here.
//! `deck.rs` draws the result.

use crate::config::{Button, Config, KEY_COUNT, Look, ValueRef, parse_color};
use crate::icons;
use crate::pages::View;

/// What one key looks like. Compared with the previous frame to redraw only changed keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct KeyFace {
    pub color: Option<[u8; 3]>,
    /// Background image file.
    pub icon: Option<String>,
    /// Icon glyph from the bundled fonts.
    pub symbol: Option<char>,
    /// Symbol color. `None`: white.
    pub symbol_color: Option<[u8; 3]>,
    pub text: Option<String>,
    /// Key held down: drawn smaller, so it looks pushed in.
    pub pressed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub brightness: u8,
    pub keys: Vec<KeyFace>,
}

/// Looks up a live value such as "spotify:title". `None` when unavailable.
pub type Values<'a> = dyn Fn(&ValueRef) -> Option<String> + 'a;

/// `pressed`: keys to draw pushed in (empty keys never are).
pub fn build(
    config: &Config,
    view: &View,
    brightness: u8,
    pressed: &[u8],
    values: &Values,
) -> Screen {
    let keys = (0..KEY_COUNT)
        .map(|key| {
            config
                .shown_button(view, key)
                .map_or_else(KeyFace::default, |button| KeyFace {
                    pressed: pressed.contains(&key),
                    ..face(button, values)
                })
        })
        .collect();
    Screen { brightness, keys }
}

/// One button's look. Order of priority: the current `look_by` state, then live values,
/// then the button's fixed fields.
pub fn face(button: &Button, values: &Values) -> KeyFace {
    let state: Option<&Look> = button
        .look_by
        .as_ref()
        .and_then(|look_by| look_by.states.get(&values(&look_by.value)?));
    let pick =
        |from_state: Option<&String>, own: &Option<String>| from_state.or(own.as_ref()).cloned();

    let label = pick(state.and_then(|s| s.label.as_ref()), &button.label);
    let text = button.display.as_ref().and_then(values).or(label);
    let symbol = pick(state.and_then(|s| s.symbol.as_ref()), &button.symbol);
    let color = pick(state.and_then(|s| s.color.as_ref()), &button.color);
    let symbol_color = pick(
        state.and_then(|s| s.symbol_color.as_ref()),
        &button.symbol_color,
    );
    let icon = button
        .icon_value
        .as_ref()
        .and_then(values)
        .or_else(|| button.icon.clone());
    KeyFace {
        color: color.and_then(|c| parse_color(&c).ok()),
        icon,
        symbol: symbol.and_then(|s| icons::glyph(&s)),
        symbol_color: symbol_color.and_then(|c| parse_color(&c).ok()),
        text,
        pressed: false,
    }
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
        short_press = "deck:open_folder f"
        label = "Folder"
        symbol = "folder"
        color = "#FF0000"
        [[button]]
        key = 2
        display = "spotify:title"
        label = "Spotify"
        icon = "default.png"
        icon_value = "spotify:cover"
        symbol = "play"
        color = "#000000"
        look_by = { value = "spotify:status", states = { playing = { color = "#00FF00", symbol = "pause", symbol_color = "#112233" } } }
        [[button]]
        folder = "f"
        key = 0
        short_press = "deck:back"
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

        let page = build(&config, &View::Page(1), 60, &[], &values(true));
        assert_eq!(page.keys[0].text.as_deref(), Some("12:34"));
        assert_eq!(page.keys[1].text.as_deref(), Some("Folder"));
        assert_eq!(page.keys[1].symbol, Some('📁'));
        assert_eq!(page.keys[1].color, Some([255, 0, 0]));
        assert_eq!(page.keys[3], KeyFace::default());

        let folder = build(&config, &View::Folder("f".into()), 60, &[], &values(true));
        assert_eq!(folder.keys[0].text.as_deref(), Some("f"));
        assert_eq!(folder.keys[1], KeyFace::default());
    }

    #[test]
    fn state_look_then_live_values_then_fixed_fields() {
        let config = Config::parse(CONFIG).unwrap();

        let playing = build(&config, &View::Page(1), 60, &[], &values(true));
        assert_eq!(
            playing.keys[2],
            KeyFace {
                color: Some([0, 255, 0]),
                icon: Some("cover.jpg".into()),
                symbol: Some('⏸'),
                symbol_color: Some([0x11, 0x22, 0x33]),
                text: Some("Song".into()),
                pressed: false
            }
        );

        let closed = build(&config, &View::Page(1), 60, &[], &values(false));
        assert_eq!(
            closed.keys[2],
            KeyFace {
                color: Some([0, 0, 0]),
                icon: Some("default.png".into()),
                symbol: Some('▶'),
                symbol_color: None,
                text: Some("Spotify".into()),
                pressed: false
            }
        );
    }
}
