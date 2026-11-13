//! `betterstreamdux`: the daemon. Drives the Stream Deck from the config file.
//!
//! Loop: read key events -> handler runs actions -> build screen -> deck redraws what changed.
//! The config file is watched: saving it (by hand or from the GUI) reloads it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use betterstreamdux::config::Config;
use betterstreamdux::{deck, handler, modules};
use elgato_streamdeck::DeviceStateUpdate;

/// How often the loop wakes up (long presses, clock), even with no key event.
const TICK: Duration = Duration::from_millis(50);
/// How often the config file is checked for changes.
const RELOAD_CHECK: Duration = Duration::from_secs(1);

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = match std::env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => Config::default_path()?,
    };
    let config = Config::load_or_create(&config_path)?;
    log::info!(
        "loaded {} buttons on {} page(s) from {}",
        config.buttons.len(),
        config.page_count(),
        config_path.display()
    );

    let mut deck = deck::Deck::connect()?;
    let mut handler = handler::Handler::new(config, modules::Registry::start_all());
    let mut watcher = Watcher::new(&config_path);

    log::info!("ready, waiting for key presses");
    loop {
        deck.draw(&handler.screen(&clock()))?;
        for event in deck.read_events(TICK)? {
            match event {
                DeviceStateUpdate::ButtonDown(key) => handler.key_down(key, Instant::now()),
                DeviceStateUpdate::ButtonUp(key) => handler.key_up(key),
                _ => {} // encoders/touch: not on the MK.2
            }
        }
        handler.tick(Instant::now());
        if watcher.changed() {
            // An invalid file keeps the current config running.
            match Config::load(&config_path) {
                Ok(config) => {
                    handler.set_config(config);
                    log::info!("config reloaded");
                }
                Err(err) => log::error!("config not reloaded: {err:#}"),
            }
        }
    }
}

/// Local time as "HH:MM", for `display = "deck:clock"`.
fn clock() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Notices when the config file is modified.
struct Watcher {
    path: PathBuf,
    modified: Option<SystemTime>,
    last_check: Instant,
}

impl Watcher {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            modified: modified(path),
            last_check: Instant::now(),
        }
    }

    /// True once after each change. Only looks at the file every `RELOAD_CHECK`.
    fn changed(&mut self) -> bool {
        if self.last_check.elapsed() < RELOAD_CHECK {
            return false;
        }
        self.last_check = Instant::now();
        let modified = modified(&self.path);
        let changed = modified != self.modified;
        self.modified = modified;
        changed
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}
