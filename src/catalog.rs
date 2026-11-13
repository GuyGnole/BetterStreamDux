//! What each module offers, as data: its actions (and their argument) and its live values.
//!
//! Used to check the config at startup and to fill the GUI's dropdowns.
//! Each module declares its own `INFO` next to its code.

use anyhow::{Context, Result, bail, ensure};

use crate::{builtin, modules};

pub struct ModuleInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub actions: Actions,
    pub values: &'static [ValueInfo],
}

pub enum Actions {
    /// Fixed action names, each with at most one argument.
    List(&'static [ActionInfo]),
    /// The action is free text: the shell module's command line.
    Command,
}

pub struct ActionInfo {
    pub name: &'static str,
    pub arg: Arg,
    pub help: &'static str,
}

/// The argument an action takes, written after its name: `go_to_page 2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg {
    None,
    /// A main page number.
    Page,
    /// A folder name.
    Folder,
    /// A numeric id, e.g. a Discord channel id.
    Id,
    /// A duration in minutes, 1 to 1440.
    Minutes,
}

pub struct ValueInfo {
    pub name: &'static str,
    pub kind: ValueKind,
    pub help: &'static str,
    /// Shown by the GUI preview instead of the real value. Empty for none.
    pub sample: &'static str,
    /// Every value it can take, for `color_by`. Empty when free (e.g. a title).
    pub states: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// Text, for `display` and `color_by`.
    Text,
    /// An image file path, for `icon_value`.
    Image,
}

// Short constructors, so module catalogs stay one line per entry.

pub const fn action(name: &'static str, help: &'static str) -> ActionInfo {
    ActionInfo {
        name,
        arg: Arg::None,
        help,
    }
}

pub const fn action_with(name: &'static str, arg: Arg, help: &'static str) -> ActionInfo {
    ActionInfo { name, arg, help }
}

pub const fn text(name: &'static str, help: &'static str, sample: &'static str) -> ValueInfo {
    ValueInfo {
        name,
        kind: ValueKind::Text,
        help,
        sample,
        states: &[],
    }
}

pub const fn state(
    name: &'static str,
    help: &'static str,
    states: &'static [&'static str],
) -> ValueInfo {
    ValueInfo {
        name,
        kind: ValueKind::Text,
        help,
        sample: states[0],
        states,
    }
}

pub const fn image(name: &'static str, help: &'static str) -> ValueInfo {
    ValueInfo {
        name,
        kind: ValueKind::Image,
        help,
        sample: "",
        states: &[],
    }
}

/// Every module, the built-in `deck` first.
pub fn all() -> impl Iterator<Item = &'static ModuleInfo> {
    std::iter::once(&builtin::INFO).chain(modules::INFOS)
}

pub fn find(module: &str) -> Option<&'static ModuleInfo> {
    all().find(|m| m.name == module)
}

impl ModuleInfo {
    pub fn action(&self, name: &str) -> Option<&ActionInfo> {
        match &self.actions {
            Actions::List(list) => list.iter().find(|a| a.name == name),
            Actions::Command => None,
        }
    }

    pub fn value(&self, name: &str) -> Option<&ValueInfo> {
        self.values.iter().find(|v| v.name == name)
    }

    /// Checks that `action` is one of this module's actions, with a valid argument.
    pub fn check_action(&self, action: &str) -> Result<()> {
        if let Actions::Command = self.actions {
            ensure!(!action.trim().is_empty(), "the command is empty");
            return Ok(());
        }
        let (name, arg) = split_action(action);
        let info = self
            .action(name)
            .with_context(|| format!("unknown {} action '{action}'", self.name))?;
        match (info.arg, arg) {
            (Arg::None, None) => {}
            (Arg::None, Some(_)) => bail!("'{name}' takes no argument"),
            (_, None | Some("")) => bail!("'{name}' needs an argument"),
            (Arg::Folder, Some(_)) => {}
            (Arg::Page, Some(page)) => {
                page.parse::<u8>()
                    .with_context(|| format!("bad page number '{page}'"))?;
            }
            (Arg::Minutes, Some(minutes)) => {
                let minutes: u32 = minutes
                    .parse()
                    .with_context(|| format!("'{minutes}' is not a number of minutes"))?;
                ensure!(
                    (1..=1440).contains(&minutes),
                    "minutes must be between 1 and 1440"
                );
            }
            (Arg::Id, Some(id)) => {
                ensure!(
                    id.chars().all(|c| c.is_ascii_digit()),
                    "'{id}' is not a numeric id"
                );
            }
        }
        Ok(())
    }
}

/// Splits `"go_to_page 3"` into `("go_to_page", Some("3"))`.
pub fn split_action(action: &str) -> (&str, Option<&str>) {
    match action.split_once(' ') {
        Some((name, arg)) => (name, Some(arg.trim())),
        None => (action, None),
    }
}

/// The reverse of [`split_action`].
pub fn join_action(name: &str, arg: Option<&str>) -> String {
    match arg {
        Some(arg) => format!("{name} {arg}"),
        None => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_names_are_unique() {
        let mut names: Vec<_> = all().map(|m| m.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all().count());
    }

    #[test]
    fn checks_actions_and_arguments() {
        let deck = find("deck").unwrap();
        assert!(deck.check_action("next_page").is_ok());
        assert!(deck.check_action("go_to_page 2").is_ok());
        assert!(deck.check_action("go_to_page").is_err());
        assert!(deck.check_action("go_to_page ").is_err());
        assert!(deck.check_action("go_to_page two").is_err());
        assert!(deck.check_action("next_page 2").is_err());
        assert!(deck.check_action("fly").is_err());
        let discord = find("discord").unwrap();
        assert!(discord.check_action("join_channel 123").is_ok());
        assert!(discord.check_action("join_channel general").is_err());
        let timer = find("timer").unwrap();
        assert!(timer.check_action("start 5").is_ok());
        assert!(timer.check_action("start 0").is_err());
        let shell = find("shell").unwrap();
        assert!(shell.check_action("notify-send hi").is_ok());
        assert!(shell.check_action(" ").is_err());
    }

    #[test]
    fn split_and_join_round_trip() {
        assert_eq!(
            split_action("open_folder my music"),
            ("open_folder", Some("my music"))
        );
        assert_eq!(
            join_action("open_folder", Some("my music")),
            "open_folder my music"
        );
        assert_eq!(join_action("back", None), "back");
    }
}
