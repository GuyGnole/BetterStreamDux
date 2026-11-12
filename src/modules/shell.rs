//! Runs any shell command. Handy for quick automations before writing a real module.
//!
//! Action: the command line itself, e.g. `"notify-send hello"`.

use std::process::Command;

use anyhow::{Context, Result};

use super::Module;

pub struct Shell;

impl Module for Shell {
    fn name(&self) -> &'static str {
        "shell"
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
