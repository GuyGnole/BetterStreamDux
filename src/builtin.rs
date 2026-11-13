//! The built-in `deck` module: actions that control the Stream Deck itself
//! (pages, folders, brightness). Executed by the handler, not by `modules/`.

use anyhow::{Context, Result, bail};

use crate::catalog::{Actions, Arg, ModuleInfo, action, action_with, text};

pub const MODULE_NAME: &str = "deck";

pub const INFO: ModuleInfo = ModuleInfo {
    name: MODULE_NAME,
    description: "The Stream Deck itself: pages, folders, brightness",
    actions: Actions::List(&[
        action("next_page", "Next main page (wraps around)"),
        action("previous_page", "Previous main page (wraps around)"),
        action_with("go_to_page", Arg::Page, "Jump to a main page"),
        action_with("open_folder", Arg::Folder, "Open a folder"),
        action("back", "Leave the folder, back to its page"),
        action("brightness_up", "Brightness +10%"),
        action("brightness_down", "Brightness -10%"),
    ]),
    values: &[
        text("clock", "Current time, HH:MM", "12:34"),
        text(
            "page_number",
            "Page number like 2/3, or the folder name",
            "1/2",
        ),
        text("brightness", "Brightness, like 60%", "60%"),
    ],
};

/// Brightness change per `brightness_up` / `brightness_down`.
const BRIGHTNESS_STEP: u8 = 10;
/// Never go fully dark: a black deck looks unplugged.
pub const BRIGHTNESS_MIN: u8 = 10;
pub const BRIGHTNESS_MAX: u8 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckAction {
    NextPage,
    PreviousPage,
    GoToPage(u8),
    OpenFolder(String),
    Back,
    BrightnessUp,
    BrightnessDown,
}

impl DeckAction {
    /// Parses an action string like `"next_page"` or `"go_to_page 3"`.
    pub fn parse(action: &str) -> Result<Self> {
        let (name, arg) = match action.split_once(' ') {
            Some((name, arg)) => (name, Some(arg.trim())),
            None => (action, None),
        };
        let action = match (name, arg) {
            ("next_page", None) => Self::NextPage,
            ("previous_page", None) => Self::PreviousPage,
            ("go_to_page", Some(n)) => Self::GoToPage(
                n.parse()
                    .with_context(|| format!("bad page number '{n}'"))?,
            ),
            ("open_folder", Some(name)) => Self::OpenFolder(name.to_string()),
            ("back", None) => Self::Back,
            ("brightness_up", None) => Self::BrightnessUp,
            ("brightness_down", None) => Self::BrightnessDown,
            _ => bail!("unknown deck action '{action}'"),
        };
        Ok(action)
    }
}

/// The deck's live values, for `display = "deck:..."`.
pub fn value(name: &str, clock: &str, page_label: &str, brightness: u8) -> Option<String> {
    match name {
        "clock" => Some(clock.to_string()),
        "page_number" => Some(page_label.to_string()),
        "brightness" => Some(format!("{brightness}%")),
        _ => None,
    }
}

pub fn brightness_up(current: u8) -> u8 {
    current.saturating_add(BRIGHTNESS_STEP).min(BRIGHTNESS_MAX)
}

pub fn brightness_down(current: u8) -> u8 {
    current.saturating_sub(BRIGHTNESS_STEP).max(BRIGHTNESS_MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_actions() {
        assert_eq!(
            DeckAction::parse("next_page").unwrap(),
            DeckAction::NextPage
        );
        assert_eq!(
            DeckAction::parse("go_to_page 3").unwrap(),
            DeckAction::GoToPage(3)
        );
        assert_eq!(
            DeckAction::parse("open_folder music").unwrap(),
            DeckAction::OpenFolder("music".into())
        );
    }

    #[test]
    fn rejects_bad_actions() {
        assert!(DeckAction::parse("go_to_page").is_err());
        assert!(DeckAction::parse("go_to_page x").is_err());
        assert!(DeckAction::parse("next_page 2").is_err());
        assert!(DeckAction::parse("fly").is_err());
    }

    #[test]
    fn every_catalog_action_parses() {
        let crate::catalog::Actions::List(actions) = INFO.actions else {
            panic!("deck actions are a list");
        };
        for info in actions {
            let arg = (info.arg != Arg::None).then_some("1");
            let action = crate::catalog::join_action(info.name, arg);
            assert!(DeckAction::parse(&action).is_ok(), "{action}");
        }
    }

    #[test]
    fn brightness_stays_in_range() {
        assert_eq!(brightness_up(95), 100);
        assert_eq!(brightness_down(15), 10);
        assert_eq!(brightness_up(50), 60);
    }
}
