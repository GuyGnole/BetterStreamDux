//! Keyboard shortcuts of the editor.
//!
//!   Ctrl+S             save
//!   Ctrl+Z             undo            Ctrl+Shift+Z / Ctrl+Y   redo
//!   Ctrl+C / X / V     copy / cut / paste the selected key
//!   Delete, Backspace  empty the selected key
//!   Arrow keys         move the selection        Escape   unselect
//!
//! While typing in a text field, only Ctrl+S is handled here: the field gets the rest
//! (its own copy/paste and undo).

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use super::Editor;
use crate::config::KEY_COUNT;

/// Keys per row on the MK.2.
const COLUMNS: u8 = 5;

pub fn handle(ed: &mut Editor, ctx: &egui::Context, can_save: bool) {
    let pressed = |modifiers, key| {
        ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
    };
    if pressed(Modifiers::COMMAND, Key::S) && can_save {
        ed.save();
    }
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    // Ctrl+Shift+Z first: Ctrl+Z also matches it.
    if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || pressed(Modifiers::COMMAND, Key::Y)
    {
        ed.redo();
    } else if pressed(Modifiers::COMMAND, Key::Z) {
        ed.undo();
    }

    // Copy/cut/paste arrive as events, not key presses (paste carries the clipboard text).
    if let Some(key) = ed.selected {
        for event in ctx.input(|i| i.events.clone()) {
            match event {
                egui::Event::Copy => ed.copy(ctx, key),
                egui::Event::Cut => ed.cut(ctx, key),
                egui::Event::Paste(text) => ed.paste_text(key, &text),
                _ => {}
            }
        }
        if pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace) {
            ed.clear_key(key);
        }
    }

    if pressed(Modifiers::NONE, Key::Escape) {
        ed.selected = None;
    }
    for (key, step) in [
        (Key::ArrowLeft, Step::Left),
        (Key::ArrowRight, Step::Right),
        (Key::ArrowUp, Step::Up),
        (Key::ArrowDown, Step::Down),
    ] {
        if pressed(Modifiers::NONE, key) {
            ed.selected = Some(ed.selected.map_or(0, |current| step.from(current)));
        }
    }
}

#[derive(Clone, Copy)]
enum Step {
    Left,
    Right,
    Up,
    Down,
}

impl Step {
    /// The key next to `key` in this direction, or `key` itself at the edge.
    fn from(self, key: u8) -> u8 {
        let column = key % COLUMNS;
        match self {
            Self::Left if column > 0 => key - 1,
            Self::Right if column < COLUMNS - 1 => key + 1,
            Self::Up if key >= COLUMNS => key - COLUMNS,
            Self::Down if key + COLUMNS < KEY_COUNT => key + COLUMNS,
            _ => key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_stay_on_the_grid() {
        assert_eq!(Step::Right.from(3), 4);
        assert_eq!(Step::Right.from(4), 4, "end of row");
        assert_eq!(Step::Left.from(5), 5, "start of row");
        assert_eq!(Step::Down.from(7), 12);
        assert_eq!(Step::Down.from(12), 12, "bottom row");
        assert_eq!(Step::Up.from(2), 2, "top row");
    }
}
