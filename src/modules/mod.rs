//! Application modules. One file per application.
//!
//! To add a module:
//!   1. create `src/modules/<name>.rs` with a struct implementing [`Module`]
//!   2. add `mod <name>;` below
//!   3. register it in [`Registry::start_all`] and its `INFO` in [`INFOS`]

mod discord;
mod shell;
mod spotify;

use std::collections::HashMap;

use anyhow::{Result, bail};

use crate::catalog::ModuleInfo;

/// What each module offers (see `catalog.rs`). Add new modules here too.
pub const INFOS: [&ModuleInfo; 3] = [&spotify::INFO, &discord::INFO, &shell::INFO];

/// Something a Stream Deck key can control.
pub trait Module {
    /// Name used in `config.toml` (`module = "..."`).
    fn name(&self) -> &'static str;

    /// Runs one action, e.g. `"play_pause"`.
    fn run(&mut self, action: &str) -> Result<()>;

    /// A live value for keys (`display = "module:name"`), e.g. `"title"`.
    /// Called every tick: must be instant, so do slow work in a background thread.
    fn value(&self, _name: &str) -> Option<String> {
        None
    }
}

/// Holds every available module, looked up by name.
pub struct Registry {
    modules: HashMap<&'static str, Box<dyn Module>>,
}

impl Registry {
    /// Every real module, with their background threads started. Add new ones here.
    pub fn start_all() -> Self {
        Self::from(vec![
            Box::new(spotify::Spotify::start()) as Box<dyn Module>,
            Box::new(discord::Discord::start()),
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

    pub fn value(&self, module: &str, name: &str) -> Option<String> {
        self.modules.get(module)?.value(name)
    }
}
