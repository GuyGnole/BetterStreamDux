//! The built-in `deck` module: actions that control the Stream Deck itself
//! (pages, folders, brightness). Executed by the handler, not by `modules/`.

use anyhow::{Context, Result, bail};

pub const MODULE_NAME: &str = "deck";

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
    fn brightness_stays_in_range() {
        assert_eq!(brightness_up(95), 100);
        assert_eq!(brightness_down(15), 10);
        assert_eq!(brightness_up(50), 60);
    }
}
