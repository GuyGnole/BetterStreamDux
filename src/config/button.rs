//! One key: where it is, how it looks, and what it does.
//!
//! Actions are written "module:action", e.g. `spotify:next` or `deck:go_to_page 2`.
//! Each trigger takes one action or a list, run in order.
//! Tools can have several independent copies, chosen by an id after the module name:
//! `timer.tea:start 4` and `timer.pasta:start 10` are two timers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::builtin::{self, DeckAction};
use crate::catalog::ValueKind;
use crate::pages::View;

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

    /// Actions when the key is released before `long_press_ms`.
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "one_or_many")]
    pub short_press: Vec<String>,
    /// Actions when the key has been held for `long_press_ms`.
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "one_or_many")]
    pub long_press: Vec<String>,
    /// Actions when the key goes down and when it comes back up (push-to-talk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub while_press: Option<WhilePress>,

    /// Icon name (see `icons.rs`) or a single emoji.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Fixed text. Also shown when `display` has no value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Live text, e.g. "deck:clock" or "spotify:title".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<ValueRef>,
    /// Background color as "#RRGGBB".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Path to a png/jpg image, drawn as the background.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Background image path from a live value, e.g. "spotify:cover". Falls back to `icon`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_value: Option<ValueRef>,
    /// Look changes depending on a live value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look_by: Option<LookBy>,

    /// Old format, converted when loading (see `migrate.rs`), never saved.
    #[serde(default, skip_serializing)]
    pub module: Option<String>,
    /// Old format, converted to `look_by` when loading, never saved.
    #[serde(default, skip_serializing)]
    pub color_by: Option<ColorBy>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhilePress {
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "one_or_many")]
    pub press: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "one_or_many")]
    pub release: Vec<String>,
}

/// A live value published by a module, written "module:name" in the config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ValueRef {
    pub module: String,
    /// Which copy of the module ("tea" in "timer.tea:remaining"). Empty: the default one.
    pub instance: String,
    pub name: String,
}

impl TryFrom<String> for ValueRef {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        match split_module(&text) {
            Some((module, _)) if module.ends_with('.') => {
                Err(format!("missing id after '.' in '{text}'"))
            }
            Some((module, name)) => {
                let (module, instance) = split_instance(module);
                Ok(Self {
                    module: module.to_string(),
                    instance: instance.to_string(),
                    name: name.to_string(),
                })
            }
            None => Err(format!(
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
        write!(
            f,
            "{}:{}",
            join_instance(&self.module, &self.instance),
            self.name
        )
    }
}

/// Changes the look while a live value has some states, e.g. a pause symbol on green
/// while `spotify:status` is "playing".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookBy {
    pub value: ValueRef,
    /// State -> look. States not listed keep the button's own look.
    #[serde(default)]
    pub states: BTreeMap<String, Look>,
}

/// What a state changes. Unset fields keep the button's own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Old format of `look_by`, colors only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorBy {
    pub value: ValueRef,
    pub colors: BTreeMap<String, String>,
}

/// Splits "spotify:next" into ("spotify", "next"). `None` without a module.
pub fn split_module(text: &str) -> Option<(&str, &str)> {
    text.split_once(':')
        .filter(|(module, rest)| !module.is_empty() && !rest.is_empty())
}

/// Splits "timer.tea" into ("timer", "tea"). Without an id: ("timer", "").
pub fn split_instance(module: &str) -> (&str, &str) {
    module.split_once('.').unwrap_or((module, ""))
}

/// The reverse of [`split_instance`].
pub fn join_instance(module: &str, instance: &str) -> String {
    if instance.is_empty() {
        module.to_string()
    } else {
        format!("{module}.{instance}")
    }
}

// serde's skip_serializing_if passes a reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// Reads one action or a list; writes a single action without brackets.
mod one_or_many {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }

    pub fn serialize<S: Serializer>(actions: &[String], serializer: S) -> Result<S::Ok, S::Error> {
        match actions {
            [one] => serializer.serialize_str(one),
            many => many.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<String>, D::Error> {
        Ok(match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(action) => vec![action],
            OneOrMany::Many(actions) => actions,
        })
    }
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

    /// Moves the button to `key` in `view`, keeping everything else.
    pub fn place(&mut self, view: &View, key: u8) {
        let empty = Self::new(view, key);
        (self.key, self.page, self.folder, self.shared) =
            (empty.key, empty.page, empty.folder, empty.shared);
    }

    /// True if the button sets nothing besides its position.
    pub fn is_empty(&self) -> bool {
        *self == Self::new(&self.view(), self.key)
    }

    /// Every action this button can run.
    pub fn actions(&self) -> impl Iterator<Item = &String> {
        let held = self
            .while_press
            .iter()
            .flat_map(|w| w.press.iter().chain(&w.release));
        self.short_press.iter().chain(&self.long_press).chain(held)
    }

    /// Every symbol this button can show.
    pub fn symbols(&self) -> impl Iterator<Item = &String> {
        let states = self.look_by.iter().flat_map(|l| l.states.values());
        self.symbol
            .iter()
            .chain(states.filter_map(|look| look.symbol.as_ref()))
    }

    /// Every color this button can show.
    pub fn colors(&self) -> impl Iterator<Item = &String> {
        let states = self.look_by.iter().flat_map(|l| l.states.values());
        self.color
            .iter()
            .chain(states.filter_map(|look| look.color.as_ref()))
    }

    /// Every live value this button reads, with the kind it needs.
    pub fn value_refs(&self) -> Vec<(&ValueRef, ValueKind)> {
        let mut refs = Vec::new();
        refs.extend(self.display.iter().map(|v| (v, ValueKind::Text)));
        refs.extend(self.icon_value.iter().map(|v| (v, ValueKind::Image)));
        refs.extend(self.look_by.iter().map(|l| (&l.value, ValueKind::Text)));
        refs
    }

    /// The button's built-in deck actions (parse errors are reported by the config check).
    pub fn deck_actions(&self) -> Vec<DeckAction> {
        self.actions()
            .filter_map(|action| match split_module(action) {
                Some((builtin::MODULE_NAME, rest)) => DeckAction::parse(rest).ok(),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_action_or_a_list() {
        let button: Button = toml::from_str(
            "key = 0\nshort_press = \"spotify:next\"\nlong_press = [\"spotify:stop\", \"deck:back\"]",
        )
        .unwrap();
        assert_eq!(button.short_press, ["spotify:next"]);
        assert_eq!(button.long_press, ["spotify:stop", "deck:back"]);

        let text = toml::to_string(&button).unwrap();
        assert!(
            text.contains("short_press = \"spotify:next\""),
            "single: no brackets"
        );
        assert!(text.contains("long_press = [\"spotify:stop\", \"deck:back\"]"));
    }

    #[test]
    fn values_and_actions_can_name_an_id() {
        let value = ValueRef::try_from("timer.tea:remaining".to_string()).unwrap();
        assert_eq!(
            (value.module.as_str(), value.instance.as_str()),
            ("timer", "tea")
        );
        assert_eq!(value.to_string(), "timer.tea:remaining");
        let plain = ValueRef::try_from("timer:remaining".to_string()).unwrap();
        assert_eq!(plain.instance, "");
        assert_eq!(split_instance("counter.coffee"), ("counter", "coffee"));
        assert_eq!(join_instance("counter", ""), "counter");
    }

    #[test]
    fn splits_module() {
        assert_eq!(split_module("shell:echo a:b"), Some(("shell", "echo a:b")));
        assert_eq!(split_module("next"), None);
        assert_eq!(split_module(":next"), None);
    }

    #[test]
    fn moving_keeps_the_content() {
        let mut button = Button {
            label: Some("x".into()),
            ..Button::new(&View::Folder("f".into()), 3)
        };
        button.place(&View::Shared, 9);
        assert_eq!(button.view(), View::Shared);
        assert_eq!((button.key, button.folder.as_deref()), (9, None));
        assert_eq!(button.label.as_deref(), Some("x"));
    }
}
