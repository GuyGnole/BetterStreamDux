//! Named icons ("symbols") drawn on keys, from the bundled Noto Emoji font (monochrome,
//! license in assets/fonts/NotoEmoji-OFL.txt).
//!
//! A `symbol` in the config is one of the names below, or any single emoji the font has.

use ab_glyph::{Font, FontRef};

pub const FONT: &[u8] = include_bytes!("../assets/fonts/NotoEmoji-Regular.ttf");

pub struct Icon {
    pub name: &'static str,
    pub glyph: char,
    /// Extra words for the editor's search.
    pub keywords: &'static str,
}

const fn icon(name: &'static str, glyph: char, keywords: &'static str) -> Icon {
    Icon {
        name,
        glyph,
        keywords,
    }
}

pub const ICONS: &[Icon] = &[
    // Media
    icon("play", '▶', "start resume music"),
    icon("pause", '⏸', "music"),
    icon("play_pause", '⏯', "toggle music"),
    icon("stop", '⏹', "music"),
    icon("next", '⏭', "skip track forward"),
    icon("previous", '⏮', "back track"),
    icon("fast_forward", '⏩', "seek forward"),
    icon("rewind", '⏪', "seek back"),
    icon("shuffle", '🔀', "random"),
    icon("repeat", '🔁', "loop"),
    icon("record", '⏺', "rec"),
    icon("eject", '⏏', ""),
    // Sound
    icon("volume_up", '🔊', "loud sound speaker"),
    icon("volume_down", '🔉', "quiet sound speaker"),
    icon("volume_low", '🔈', "sound speaker"),
    icon("mute", '🔇', "silent sound off"),
    icon("mic", '🎤', "microphone voice talk"),
    icon("studio_mic", '🎙', "microphone podcast voice"),
    icon("headphones", '🎧', "headset deafen listen"),
    icon("music", '🎵', "note song"),
    icon("notes", '🎶', "music song"),
    icon("radio", '📻', "music"),
    icon("bell", '🔔', "notification alert"),
    icon("bell_off", '🔕', "notification silent"),
    // Time
    icon("stopwatch", '⏱', "chrono time"),
    icon("timer", '⏲', "countdown time"),
    icon("alarm", '⏰', "clock wake ring"),
    icon("hourglass", '⏳', "wait time"),
    icon("clock", '🕒', "time hour"),
    icon("calendar", '📅', "date day"),
    icon("tomato", '🍅', "pomodoro focus"),
    // Navigation
    icon("arrow_left", '⬅', "previous page back"),
    icon("arrow_right", '➡', "next page forward"),
    icon("arrow_up", '⬆', ""),
    icon("arrow_down", '⬇', ""),
    icon("back", '🔙', "return"),
    icon("home", '🏠', "house start"),
    icon("folder", '📁', "directory"),
    icon("open_folder", '📂', "directory"),
    icon("refresh", '🔄', "reload sync"),
    // Controls
    icon("plus", '➕', "add increment more"),
    icon("minus", '➖', "remove decrement less"),
    icon("check", '✅', "ok done yes"),
    icon("cross", '❌', "cancel no close"),
    icon("gear", '⚙', "settings config"),
    icon("wrench", '🔧', "tool settings"),
    icon("lock", '🔒', "secure"),
    icon("unlock", '🔓', "open"),
    icon("key", '🔑', "password"),
    icon("search", '🔍', "find"),
    icon("link", '🔗', "url"),
    icon("pin", '📌', ""),
    icon("warning", '⚠', "alert"),
    icon("info", 'ℹ', "information"),
    icon("question", '❓', "help"),
    // Light and display
    icon("sun", '☀', "day bright"),
    icon("moon", '🌙', "night dark"),
    icon("brightness_up", '🔆', "light bright"),
    icon("brightness_down", '🔅', "light dim"),
    icon("bulb", '💡', "light idea"),
    icon("star", '⭐', "favorite"),
    icon("fire", '🔥', "hot"),
    icon("sparkles", '✨', "magic"),
    // Communication
    icon("chat", '💬', "message discord talk"),
    icon("phone", '📞', "call"),
    icon("mail", '📧', "email"),
    icon("megaphone", '📣', "announce"),
    icon("people", '👥', "group users channel"),
    icon("person", '👤', "user"),
    icon("globe", '🌐', "web internet"),
    icon("wave", '👋', "hello"),
    // Devices and apps
    icon("computer", '🖥', "desktop screen"),
    icon("laptop", '💻', "computer"),
    icon("keyboard", '⌨', "type terminal command"),
    icon("mouse", '🖱', ""),
    icon("gamepad", '🎮', "game controller"),
    icon("tv", '📺', "screen"),
    icon("camera", '📷', "photo picture"),
    icon("video", '📹', "camera record"),
    icon("film", '🎬', "movie clap"),
    icon("battery", '🔋', "power"),
    icon("phone_mobile", '📱', "smartphone"),
    // Work
    icon("pencil", '✏', "edit write"),
    icon("note", '📝', "memo write"),
    icon("clipboard", '📋', "copy paste"),
    icon("book", '📖', "read"),
    icon("chart", '📈', "stats graph"),
    icon("save", '💾', "floppy disk"),
    icon("trash", '🗑', "delete bin"),
    icon("coffee", '☕', "break drink"),
    icon("rocket", '🚀', "launch start"),
    icon("target", '🎯', "goal focus"),
    icon("trophy", '🏆', "win"),
    icon("heart", '❤', "love like"),
    icon("thumbs_up", '👍', "like yes"),
    icon("thumbs_down", '👎', "dislike no"),
    icon("dice", '🎲', "random game"),
    icon("art", '🎨', "paint color"),
];

/// The character to draw for `symbol`: an icon name, or any single emoji the font has.
pub fn glyph(symbol: &str) -> Option<char> {
    if let Some(icon) = ICONS.iter().find(|i| i.name == symbol) {
        return Some(icon.glyph);
    }
    // Emoji typed by hand often carry an invisible "emoji style" selector.
    let mut chars = symbol.trim_end_matches('\u{FE0F}').chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if font().glyph_id(c).0 != 0 => Some(c),
        _ => None,
    }
}

pub fn font() -> FontRef<'static> {
    FontRef::try_from_slice(FONT).expect("bundled font is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_in_the_font_and_named_once() {
        let font = font();
        let mut names = Vec::new();
        for icon in ICONS {
            assert_ne!(font.glyph_id(icon.glyph).0, 0, "{} has no glyph", icon.name);
            names.push(icon.name);
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ICONS.len(), "duplicate icon name");
    }

    #[test]
    fn finds_names_and_raw_emoji() {
        assert_eq!(glyph("play"), Some('▶'));
        assert_eq!(glyph("🦊"), Some('🦊'));
        assert_eq!(glyph("⚙\u{FE0F}"), Some('⚙'));
        assert_eq!(glyph("nope"), None);
        assert_eq!(glyph("ab"), None);
    }
}
