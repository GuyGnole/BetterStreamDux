//! Runs any shell command. Handy for quick automations before writing a real module.
//!
//! Action: the command line itself, e.g. `"notify-send hello"`.

use std::process::Command;

use anyhow::{Context, Result};

use super::Module;
use crate::catalog::{Actions, ModuleInfo};

pub const INFO: ModuleInfo = ModuleInfo {
    name: "shell",
    description: "Runs any command line",
    actions: Actions::Command,
    values: &[],
};

pub struct Shell;

impl Module for Shell {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, action: &str) -> Result<()> {
        // spawn() does not wait: a slow command never blocks the deck.
        Command::new("sh")
            .args(["-c", action])
            .spawn()
            .with_context(|| format!("cannot run '{action}'"))?;
        Ok(())
    }
}
