//! Runs any shell command. Handy for quick automations before writing a real module.
//!
//! Action: the command line itself, e.g. `"notify-send hello"`.

use std::process::Command;

use anyhow::{Context, Result};

use super::Module;
use crate::catalog::{Actions, ModuleInfo, preset};

pub const INFO: ModuleInfo = ModuleInfo {
    name: "shell",
    description: "Runs any command line",
    actions: Actions::Command,
    values: &[],
    instances: false,
    presets: &[preset(
        "Run command",
        r##"
symbol = "keyboard"
label = "Run"
short_press = "shell:notify-send 'BetterStreamDux' 'Hello'"
color = "#444444"
"##,
    )],
};

pub struct Shell;

impl Module for Shell {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, _instance: &str, action: &str) -> Result<()> {
        // spawn() does not wait: a slow command never blocks the deck.
        Command::new("sh")
            .args(["-c", action])
            .spawn()
            .with_context(|| format!("cannot run '{action}'"))?;
        Ok(())
    }
}
