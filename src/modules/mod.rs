//! Application modules. One file per application.
//!
//! To add a module:
//!   1. create `src/modules/<name>.rs` with a struct implementing [`Module`]
//!   2. add `mod <name>;` below
//!   3. register it in [`Registry::new`]

mod discord;
mod shell;
mod spotify;

use std::collections::HashMap;

use anyhow::{Result, bail};

/// Something a Stream Deck key can control.
pub trait Module {
    /// Name used in `config.toml` (`module = "..."`).
    fn name(&self) -> &'static str;

    /// Runs one action, e.g. `"play_pause"`.
    fn run(&mut self, action: &str) -> Result<()>;
}

/// Holds every available module, looked up by name.
pub struct Registry {
    modules: HashMap<&'static str, Box<dyn Module>>,
}

impl Registry {
    /// Every real module. Add new ones here.
    pub fn new() -> Self {
        Self::from(vec![
            Box::new(spotify::Spotify) as Box<dyn Module>,
            Box::new(discord::Discord),
            Box::new(shell::Shell),
        ])
    }

    /// A registry with only the given modules (used by tests).
    pub fn from(all: Vec<Box<dyn Module>>) -> Self {
        let modules = all.into_iter().map(|m| (m.name(), m)).collect();
        Self { modules }
    }

    pub fn run(&mut self, module: &str, action: &str) -> Result<()> {
        match self.modules.get_mut(module) {
            Some(m) => m.run(action),
            None => bail!("unknown module '{module}'"),
        }
    }
}
