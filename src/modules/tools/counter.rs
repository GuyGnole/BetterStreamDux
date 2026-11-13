//! Counters, one per id (`counter.coffee:increment`, `display = "counter.coffee:count"`),
//! kept across restarts in `~/.local/state/betterstreamdux/counters.toml`.
//!
//! Actions: `increment`, `decrement`, `reset`.
//! Values:  `count`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::catalog::{Actions, ModuleInfo, action, preset, text};
use crate::modules::Module;
use crate::paths;

pub const INFO: ModuleInfo = ModuleInfo {
    name: "counter",
    description: "A counter, kept across restarts",
    actions: Actions::List(&[
        action("increment", "+1"),
        action("decrement", "-1"),
        action("reset", "Back to 0"),
    ]),
    values: &[text("count", "The count", "3")],
    instances: true,
    presets: &[
        preset(
            "Counter",
            r##"
display = "counter:count"
long_press = "counter:reset"
color = "#222222"
"##,
        ),
        preset(
            "Counter +",
            r##"
symbol = "plus"
short_press = "counter:increment"
color = "#333333"
"##,
        ),
        preset(
            "Counter -",
            r##"
symbol = "minus"
short_press = "counter:decrement"
color = "#333333"
"##,
        ),
    ],
};

/// Every counter, by id ("" for the default one).
pub struct Counters {
    counts: BTreeMap<String, i64>,
}

impl Counters {
    /// Loads the saved counts (none saved: all at 0).
    pub fn load() -> Self {
        let counts = file()
            .and_then(|path| Ok(fs::read_to_string(path)?))
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_else(old_single_count);
        Self { counts }
    }

    fn save(&self) -> Result<()> {
        let path = file()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, toml::to_string(&self.counts)?)
            .with_context(|| format!("cannot save {}", path.display()))
    }
}

/// Before ids, one count was saved in `counter.txt`: it becomes the default counter.
fn old_single_count() -> BTreeMap<String, i64> {
    let old = paths::state_dir().map(|dir| dir.join("counter.txt"));
    let count = old
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| text.trim().parse().ok());
    count.map(|c| (String::new(), c)).into_iter().collect()
}

/// The new count after `action`.
fn apply(count: i64, action: &str) -> Result<i64> {
    Ok(match action {
        "increment" => count.saturating_add(1),
        "decrement" => count.saturating_sub(1),
        "reset" => 0,
        _ => bail!("unknown counter action '{action}'"),
    })
}

fn file() -> Result<PathBuf> {
    Ok(paths::state_dir()?.join("counters.toml"))
}

impl Module for Counters {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, instance: &str, action: &str) -> Result<()> {
        let count = self.counts.get(instance).copied().unwrap_or(0);
        self.counts
            .insert(instance.to_string(), apply(count, action)?);
        self.save()
    }

    fn value(&self, instance: &str, name: &str) -> Option<String> {
        let count = self.counts.get(instance).copied().unwrap_or(0);
        (name == "count").then(|| count.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_saved_by_id() {
        let counts = BTreeMap::from([(String::new(), 3), ("coffee".to_string(), 2)]);
        let text = toml::to_string(&counts).unwrap();
        let back: BTreeMap<String, i64> = toml::from_str(&text).unwrap();
        assert_eq!(back, counts);
    }

    #[test]
    fn counts() {
        assert_eq!(apply(3, "increment").unwrap(), 4);
        assert_eq!(apply(3, "decrement").unwrap(), 2);
        assert_eq!(apply(3, "reset").unwrap(), 0);
        assert!(apply(3, "double").is_err());
    }
}
