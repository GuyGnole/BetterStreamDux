//! A counter, kept across restarts in `~/.local/state/betterstreamdux/counter.txt`.
//!
//! Actions: `increment`, `decrement`, `reset`.
//! Values:  `count`.

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

pub struct Counter {
    count: i64,
}

impl Counter {
    /// Loads the saved count (0 if there is none).
    pub fn load() -> Self {
        let count = file()
            .and_then(|path| Ok(fs::read_to_string(path)?))
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0);
        Self { count }
    }

    fn save(&self) -> Result<()> {
        let path = file()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, self.count.to_string())
            .with_context(|| format!("cannot save {}", path.display()))
    }
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
    Ok(paths::state_dir()?.join("counter.txt"))
}

impl Module for Counter {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, action: &str) -> Result<()> {
        self.count = apply(self.count, action)?;
        self.save()
    }

    fn value(&self, name: &str) -> Option<String> {
        (name == "count").then(|| self.count.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        assert_eq!(apply(3, "increment").unwrap(), 4);
        assert_eq!(apply(3, "decrement").unwrap(), 2);
        assert_eq!(apply(3, "reset").unwrap(), 0);
        assert!(apply(3, "double").is_err());
    }
}
