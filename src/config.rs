//! The config file: which key does what. Loading, saving, and checking it for mistakes.
//!
//! Default location: `~/.config/betterstreamdux/config.toml`, created from
//! `config.example.toml` on first run. Edited by hand or with the GUI.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::builtin::{self, DeckAction};
use crate::catalog::{self, ValueKind};
use crate::pages::View;
use crate::paths;

/// Keys on a Stream Deck MK.2.
pub const KEY_COUNT: u8 = 15;
pub const MAX_PAGES: u8 = 5;

/// Written to the default location on first run.
const EXAMPLE: &str = include_str!("../config.example.toml");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Brightness at startup, 10-100.
    #[serde(default = "default_brightness")]
    pub brightness: u8,

    /// How long a key must be held to count as a long press.
    #[serde(default = "default_long_press_ms")]
    pub long_press_ms: u64,

    /// Number of main pages. Optional: pages used by buttons always count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<u8>,

    #[serde(default)]
    pub pomodoro: PomodoroSettings,

    #[serde(default, rename = "button")]
    pub buttons: Vec<Button>,
}

/// Durations of the pomodoro module, in minutes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PomodoroSettings {
    pub work_minutes: u32,
    pub break_minutes: u32,
    pub long_break_minutes: u32,
    /// A long break comes after this many work rounds.
    pub rounds: u32,
}

impl Default for PomodoroSettings {
    fn default() -> Self {
        Self {
            work_minutes: 25,
            break_minutes: 5,
            long_break_minutes: 15,
            rounds: 4,
        }
    }
}

/// One key binding on one page or folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    /// Key index, 0-14.
    pub key: u8,
    /// Main page 1-5. Defaults to 1 when `folder` is not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u8>,
    /// Folder name, instead of `page`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Shown on every main page, instead of `page`/`folder`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub shared: bool,

    /// Module name, e.g. "spotify", or "deck" for built-in actions. Needed if a trigger is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Action when the key is released before `long_press_ms`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_press: Option<String>,
    /// Action when the key has been held for `long_press_ms`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_press: Option<String>,
    /// Actions when the key goes down and when it comes back up (push-to-talk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub while_press: Option<WhilePress>,

    /// Fixed text drawn on the key. Also shown when `display` has no value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Live text drawn on the key, e.g. "deck:clock" or "spotify:title".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<ValueRef>,
    /// Background color as "#RRGGBB".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Background color picked from a live value. Falls back to `color`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_by: Option<ColorBy>,
    /// Path to a png/jpg image. Takes priority over colors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Image path from a live value, e.g. "spotify:cover". Falls back to `icon`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_value: Option<ValueRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhilePress {
    pub press: String,
    pub release: String,
}

/// A live value published by a module, written "module:name" in the config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ValueRef {
    pub module: String,
    pub name: String,
}

impl TryFrom<String> for ValueRef {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        match text.split_once(':') {
            Some((module, name)) if !module.is_empty() && !name.is_empty() => Ok(Self {
                module: module.to_string(),
                name: name.to_string(),
            }),
            _ => Err(format!(
                "'{text}' should look like module:value, e.g. spotify:title"
            )),
        }
    }
}

impl From<ValueRef> for String {
    fn from(value: ValueRef) -> Self {
        value.to_string()
    }
}

impl std::fmt::Display for ValueRef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.module, self.name)
    }
}

/// Picks the key color from a live value, e.g. green while `spotify:status` is "playing".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorBy {
    pub value: ValueRef,
    /// Value -> "#RRGGBB". Values not listed use the button's `color`.
    pub colors: BTreeMap<String, String>,
}

// serde's skip_serializing_if passes a reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

fn default_brightness() -> u8 {
    60
}

fn default_long_press_ms() -> u64 {
    500
}

impl Button {
    /// An empty button at `key` in `view`.
    pub fn new(view: &View, key: u8) -> Self {
        let (page, folder, shared) = match view {
            View::Page(page) => (Some(*page), None, false),
            View::Folder(name) => (None, Some(name.clone()), false),
            View::Shared => (None, None, true),
        };
        Self {
            key,
            page,
            folder,
            shared,
            ..Self::default()
        }
    }

    /// The page, folder or shared layer this button belongs to.
    pub fn view(&self) -> View {
        match &self.folder {
            _ if self.shared => View::Shared,
            Some(name) => View::Folder(name.clone()),
            None => View::Page(self.page.unwrap_or(1)),
        }
    }

    /// True if the button sets nothing besides its position.
    pub fn is_empty(&self) -> bool {
        *self == Self::new(&self.view(), self.key)
    }

    /// Every action string this button can run.
    pub fn actions(&self) -> Vec<&str> {
        let mut actions = Vec::new();
        actions.extend(self.short_press.as_deref());
        actions.extend(self.long_press.as_deref());
        if let Some(w) = &self.while_press {
            actions.extend([w.press.as_str(), w.release.as_str()]);
        }
        actions
    }

    /// Every live value this button reads, with the kind it needs.
    fn value_refs(&self) -> Vec<(&ValueRef, ValueKind)> {
        let mut refs = Vec::new();
        refs.extend(self.display.iter().map(|v| (v, ValueKind::Text)));
        refs.extend(self.icon_value.iter().map(|v| (v, ValueKind::Image)));
        refs.extend(self.color_by.iter().map(|c| (&c.value, ValueKind::Text)));
        refs
    }

    fn deck_actions(&self) -> Vec<DeckAction> {
        if self.module.as_deref() != Some(builtin::MODULE_NAME) {
            return Vec::new();
        }
        // Parse errors are reported by `validate_button`.
        self.actions()
            .into_iter()
            .filter_map(|a| DeckAction::parse(a).ok())
            .collect()
    }
}

impl Config {
    /// `~/.config/betterstreamdux/config.toml`.
    pub fn default_path() -> Result<PathBuf> {
        Ok(paths::config_dir()?.join("config.toml"))
    }

    /// Loads `path`, creating it from the example config if it does not exist yet.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if !path.exists() {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(path, EXAMPLE)
                .with_context(|| format!("cannot create {}", path.display()))?;
            log::info!("created {} from the example config", path.display());
        }
        Self::load(path)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid config in {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text)?;
        config.check()?;
        Ok(config)
    }

    /// Writes the config. Goes through a temporary file so the daemon never reads half a file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let temporary = path.with_extension("toml.tmp");
        fs::write(&temporary, self.to_toml()?)
            .with_context(|| format!("cannot write {}", temporary.display()))?;
        fs::rename(&temporary, path).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(())
    }

    /// The config as TOML text, buttons sorted by page/folder then key.
    pub fn to_toml(&self) -> Result<String> {
        let mut sorted = self.clone();
        // Shared keys first, then pages, then folders.
        sorted.buttons.sort_by_key(|b| {
            let group = if b.shared {
                0
            } else if b.folder.is_some() {
                2
            } else {
                1
            };
            (group, b.folder.clone(), b.page.unwrap_or(1), b.key)
        });
        Ok(toml::to_string(&sorted)?)
    }

    /// The button the deck shows at `key` in `view`: the view's own button, or on a
    /// main page, the shared one.
    pub fn shown_button(&self, view: &View, key: u8) -> Option<&Button> {
        self.button(view, key).or_else(|| match view {
            View::Page(_) => self.button(&View::Shared, key),
            View::Folder(_) | View::Shared => None,
        })
    }

    /// True if `key` on this main page shows the shared key (the page has none of its own).
    pub fn shows_shared(&self, view: &View, key: u8) -> bool {
        matches!(view, View::Page(_))
            && self.button(view, key).is_none()
            && self.button(&View::Shared, key).is_some()
    }

    /// The button defined at `key` in `view` itself, if any (see also `shown_button`).
    pub fn button(&self, view: &View, key: u8) -> Option<&Button> {
        self.buttons
            .iter()
            .find(|b| b.key == key && b.view() == *view)
    }

    /// Number of main pages: `pages`, or the highest page used if higher.
    pub fn page_count(&self) -> u8 {
        let used = self.buttons.iter().filter_map(|b| b.page).max();
        used.unwrap_or(1).max(self.pages.unwrap_or(1))
    }

    /// Folder names, sorted.
    pub fn folders(&self) -> Vec<String> {
        let mut folders: Vec<String> = self
            .buttons
            .iter()
            .filter_map(|b| b.folder.clone())
            .collect();
        folders.sort();
        folders.dedup();
        folders
    }

    /// Catches config mistakes at startup (and live in the GUI) instead of when a key is pressed.
    pub fn check(&self) -> Result<()> {
        ensure!(
            (builtin::BRIGHTNESS_MIN..=builtin::BRIGHTNESS_MAX).contains(&self.brightness),
            "brightness must be between {} and {}",
            builtin::BRIGHTNESS_MIN,
            builtin::BRIGHTNESS_MAX
        );
        if let Some(pages) = self.pages {
            ensure!(
                (1..=MAX_PAGES).contains(&pages),
                "pages must be between 1 and {MAX_PAGES}"
            );
        }
        let p = &self.pomodoro;
        for (name, minutes) in [
            ("work_minutes", p.work_minutes),
            ("break_minutes", p.break_minutes),
            ("long_break_minutes", p.long_break_minutes),
        ] {
            ensure!(
                (1..=600).contains(&minutes),
                "pomodoro {name} must be between 1 and 600"
            );
        }
        ensure!(
            (1..=20).contains(&p.rounds),
            "pomodoro rounds must be between 1 and 20"
        );

        let mut seen = HashSet::new();
        for button in &self.buttons {
            let place = match button.view() {
                View::Shared => format!("shared key {}", button.key),
                View::Folder(name) => format!("folder '{name}' key {}", button.key),
                View::Page(page) => format!("page {page} key {}", button.key),
            };
            ensure!(
                seen.insert((button.view(), button.key)),
                "{place} is bound twice"
            );
            self.validate_button(button).with_context(|| place)?;
        }
        for folder in self.folders() {
            let has_back = self.buttons.iter().any(|b| {
                b.folder.as_deref() == Some(folder.as_str())
                    && b.deck_actions().contains(&DeckAction::Back)
            });
            ensure!(
                has_back,
                "folder '{folder}' has no key with the deck action 'back'"
            );
        }
        Ok(())
    }

    fn validate_button(&self, b: &Button) -> Result<()> {
        ensure!(
            b.key < KEY_COUNT,
            "key must be between 0 and {}",
            KEY_COUNT - 1
        );
        let places = [b.page.is_some(), b.folder.is_some(), b.shared];
        ensure!(
            places.iter().filter(|&&set| set).count() <= 1,
            "use only one of page, folder or shared"
        );
        if let Some(page) = b.page {
            ensure!(
                (1..=MAX_PAGES).contains(&page),
                "page must be between 1 and {MAX_PAGES}"
            );
        }
        if let Some(color) = &b.color {
            parse_color(color)?;
        }
        for color in b.color_by.iter().flat_map(|c| c.colors.values()) {
            parse_color(color)?;
        }
        for (value, kind) in b.value_refs() {
            let module = catalog::find(&value.module)
                .with_context(|| format!("unknown module '{}' in '{value}'", value.module))?;
            let info = module
                .value(&value.name)
                .with_context(|| format!("unknown value '{value}'"))?;
            ensure!(
                info.kind == kind,
                "'{value}' is {} but is used as {}",
                kind_name(info.kind),
                kind_name(kind)
            );
        }

        let has_press = b.short_press.is_some() || b.long_press.is_some();
        ensure!(
            b.while_press.is_none() || !has_press,
            "while_press cannot be combined with short_press/long_press"
        );
        let has_trigger = has_press || b.while_press.is_some();
        ensure!(
            !has_trigger || b.module.is_some(),
            "a trigger needs a module"
        );

        if let Some(module) = &b.module {
            let info =
                catalog::find(module).with_context(|| format!("unknown module '{module}'"))?;
            for action in b.actions() {
                info.check_action(action)?;
            }
        }
        if b.module.as_deref() == Some(builtin::MODULE_NAME) {
            for action in b.actions() {
                self.validate_deck_action(b, &DeckAction::parse(action)?)?;
            }
        }
        Ok(())
    }

    fn validate_deck_action(&self, b: &Button, action: &DeckAction) -> Result<()> {
        match action {
            DeckAction::GoToPage(page) => ensure!(
                (1..=self.page_count()).contains(page),
                "go_to_page {page}: there is no page {page}"
            ),
            DeckAction::OpenFolder(name) => {
                ensure!(b.folder.is_none(), "folders cannot open other folders");
                ensure!(
                    self.folders().contains(name),
                    "folder '{name}' does not exist"
                );
            }
            _ => {}
        }
        Ok(())
    }
}

fn kind_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Text => "text",
        ValueKind::Image => "an image",
    }
}

/// Turns "#RRGGBB" into `[r, g, b]`.
pub fn parse_color(hex: &str) -> Result<[u8; 3]> {
    let digits = hex.trim_start_matches('#');
    if digits.len() != 6 || !digits.is_ascii() {
        bail!("color must look like #RRGGBB, got '{hex}'");
    }
    let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16);
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

/// Turns `[r, g, b]` into "#RRGGBB".
pub fn format_color([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid button, to append fields to.
    const OK: &str = "[[button]]\nkey = 0\nmodule = \"shell\"\nshort_press = \"a\"\n";

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
            page = 2
            key = 1
            module = "discord"
            while_press = { press = "unmute", release = "mute" }
            "#,
        )
        .unwrap();
        assert_eq!(config.brightness, 30);
        assert_eq!(config.long_press_ms, 500);
        assert_eq!(config.page_count(), 2);
        let page1 = config.button(&View::Page(1), 1).unwrap();
        assert_eq!(page1.long_press.as_deref(), Some("next"));
        let page2 = config.button(&View::Page(2), 1).unwrap();
        assert_eq!(page2.while_press.as_ref().unwrap().release, "mute");
        assert!(config.button(&View::Page(1), 3).is_none());
    }

    #[test]
    fn same_key_on_different_pages_is_fine() {
        assert!(Config::parse(&format!("{OK}{OK}page = 2\n")).is_ok());
    }

    #[test]
    fn rejects_invalid_buttons() {
        let cases = [
            ("duplicate key", format!("{OK}{OK}")),
            ("key out of range", OK.replace("key = 0", "key = 15")),
            ("page out of range", format!("{OK}page = 6\n")),
            ("page and folder", format!("{OK}page = 1\nfolder = \"f\"\n")),
            ("shared and page", format!("{OK}shared = true\npage = 2\n")),
            ("bad color", format!("{OK}color = \"red\"\n")),
            ("unknown field", format!("{OK}action = \"a\"\n")),
            ("unknown module", OK.replace("shell", "fly")),
            ("unknown action", OK.replace("shell", "spotify")),
            (
                "bad color_by color",
                format!(
                    "{OK}color_by = {{ value = \"spotify:status\", colors = {{ a = \"red\" }} }}\n"
                ),
            ),
            ("value without module", format!("{OK}display = \"clock\"\n")),
            ("unknown value", format!("{OK}display = \"deck:weather\"\n")),
            (
                "image as text",
                format!("{OK}display = \"spotify:cover\"\n"),
            ),
            (
                "text as image",
                format!("{OK}icon_value = \"spotify:title\"\n"),
            ),
            (
                "trigger without module",
                "[[button]]\nkey = 0\nshort_press = \"a\"\n".to_string(),
            ),
            (
                "while_press mixed",
                format!("{OK}while_press = {{ press = \"b\", release = \"c\" }}\n"),
            ),
        ];
        for (name, text) in cases {
            assert!(Config::parse(&text).is_err(), "should reject: {name}");
        }
    }

    #[test]
    fn live_look_without_action_is_valid() {
        let config = Config::parse(
            r##"
            [[button]]
            key = 0
            display = "spotify:title"
            icon_value = "spotify:cover"
            color_by = { value = "spotify:status", colors = { playing = "#1DB954" } }
            "##,
        )
        .unwrap();
        let button = &config.buttons[0];
        assert_eq!(button.display.as_ref().unwrap().module, "spotify");
        assert_eq!(button.display.as_ref().unwrap().name, "title");
    }

    #[test]
    fn checks_deck_actions() {
        let deck = |action: &str, extra: &str| {
            format!("[[button]]\nkey = 0\nmodule = \"deck\"\nshort_press = \"{action}\"\n{extra}")
        };
        let back_in_f = deck("back", "folder = \"f\"\n");

        assert!(Config::parse(&deck("fly", "")).is_err(), "unknown action");
        assert!(
            Config::parse(&deck("go_to_page 2", "")).is_err(),
            "page 2 does not exist"
        );
        assert!(
            Config::parse(&format!("pages = 2\n{}", deck("go_to_page 2", ""))).is_ok(),
            "page 2 declared with pages = 2"
        );
        assert!(
            Config::parse(&deck("open_folder f", "")).is_err(),
            "folder f missing"
        );
        assert!(Config::parse(&format!("{}{back_in_f}", deck("open_folder f", ""))).is_ok());
        assert!(
            Config::parse(&deck("next_page", "folder = \"f\"\n")).is_err(),
            "folder without back"
        );
        let nested = deck("open_folder f", "folder = \"f\"\nkey = 1\n").replace("key = 0\n", "");
        assert!(
            Config::parse(&format!("{back_in_f}{nested}")).is_err(),
            "folder in folder"
        );
    }

    #[test]
    fn example_config_is_valid() {
        Config::parse(EXAMPLE).unwrap();
    }

    #[test]
    fn save_and_load_give_the_same_config() {
        let config = Config::parse(EXAMPLE).unwrap();
        let reloaded = Config::parse(&config.to_toml().unwrap()).unwrap();
        assert_eq!(reloaded.to_toml().unwrap(), config.to_toml().unwrap());
    }

    #[test]
    fn page_key_wins_over_shared_key() {
        let own = OK.replace("short_press = \"a\"", "short_press = \"own\"");
        let config =
            Config::parse(&format!("pages = 2\n{OK}shared = true\n{own}page = 2\n")).unwrap();
        let shown = |page| config.shown_button(&View::Page(page), 0).unwrap();
        assert!(shown(1).shared, "page 1 has no key 0: the shared one shows");
        assert_eq!(shown(2).short_press.as_deref(), Some("own"));
        assert!(config.shown_button(&View::Folder("f".into()), 0).is_none());
    }

    #[test]
    fn empty_button_is_detected() {
        let mut button = Button::new(&View::Folder("f".into()), 3);
        assert!(button.is_empty());
        button.label = Some("x".into());
        assert!(!button.is_empty());
    }

    #[test]
    fn colors_round_trip() {
        assert_eq!(parse_color("#1DB954").unwrap(), [0x1D, 0xB9, 0x54]);
        assert_eq!(format_color([0x1D, 0xB9, 0x54]), "#1DB954");
        assert!(parse_color("#123").is_err());
    }
}
