//! Application modules. One file per application.
//!
//! To add a module:
//!   1. create `src/modules/<name>.rs` with a struct implementing [`Module`]
//!   2. add `mod <name>;` below
//!   3. register it in [`Registry::start_all`] and its `INFO` in [`INFOS`]

mod discord;
mod shell;
mod spotify;
mod tools;

use std::collections::HashMap;

use anyhow::{Result, bail};

use crate::catalog::ModuleInfo;
use crate::config::Config;

/// What each module offers (see `catalog.rs`). Add new modules here too.
pub const INFOS: [&ModuleInfo; 7] = [
    &spotify::INFO,
    &discord::INFO,
    &shell::INFO,
    &tools::timer::INFO,
    &tools::stopwatch::INFO,
    &tools::pomodoro::INFO,
    &tools::counter::INFO,
];

/// Something a Stream Deck key can control.
pub trait Module {
    /// Name used in `config.toml` (`module = "..."`).
    fn name(&self) -> &'static str;

    /// Runs one action, e.g. `"play_pause"`, on one copy of the module: `instance` is the
    /// id after the module name ("tea" in `timer.tea:start 4`), empty for the default copy.
    /// Modules without ids ignore it (the config check rejects ids for them).
    fn run(&mut self, instance: &str, action: &str) -> Result<()>;

    /// A live value for keys (`display = "module:name"`), e.g. `"title"`.
    /// Called every tick: must be instant, so do slow work in a background thread.
    fn value(&self, _instance: &str, _name: &str) -> Option<String> {
        None
    }

    /// Called every tick (50 ms), for modules that change over time. Must be instant.
    fn tick(&mut self) {}

    /// Called with the config at startup and after each reload, for module settings.
    fn configure(&mut self, _config: &Config) {}
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
            Box::new(tools::Instances::<tools::timer::Timer>::new(
                tools::timer::INFO.name,
            )),
            Box::new(tools::Instances::<tools::stopwatch::Stopwatch>::new(
                tools::stopwatch::INFO.name,
            )),
            Box::new(tools::Instances::<tools::pomodoro::Pomodoro>::new(
                tools::pomodoro::INFO.name,
            )),
            Box::new(tools::counter::Counters::load()),
        ])
    }

    /// A registry with only the given modules (used by tests).
    pub fn from(all: Vec<Box<dyn Module>>) -> Self {
        let modules = all.into_iter().map(|m| (m.name(), m)).collect();
        Self { modules }
    }

    pub fn run(&mut self, module: &str, instance: &str, action: &str) -> Result<()> {
        match self.modules.get_mut(module) {
            Some(m) => m.run(instance, action),
            None => bail!("unknown module '{module}'"),
        }
    }

    pub fn value(&self, module: &str, instance: &str, name: &str) -> Option<String> {
        self.modules.get(module)?.value(instance, name)
    }

    pub fn tick(&mut self) {
        self.modules.values_mut().for_each(|m| m.tick());
    }

    pub fn configure(&mut self, config: &Config) {
        self.modules.values_mut().for_each(|m| m.configure(config));
    }
}
