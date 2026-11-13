//! `betterstreamdux`: the daemon. Drives the Stream Deck from the config file.
//!
//! Loop: read key events -> handler runs actions -> build screen -> deck redraws what changed.
//! The config file is watched: saving it (by hand or from the GUI) reloads it.
//! The deck can be unplugged and plugged back at any time: the daemon waits for it,
//! and modules keep running meanwhile. Meant to run as a systemd user service (see README).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use betterstreamdux::config::Config;
use betterstreamdux::deck::Deck;
use betterstreamdux::handler::Handler;
use betterstreamdux::modules;
use elgato_streamdeck::DeviceStateUpdate;

/// How often the loop wakes up (long presses, clock), even with no key event.
const TICK: Duration = Duration::from_millis(50);
/// How often the config file is checked for changes.
const RELOAD_CHECK: Duration = Duration::from_secs(1);
/// How often we look for the deck while it is unplugged.
const RETRY: Duration = Duration::from_secs(1);

fn main() -> Result<()> {
    // zbus and tracing log every D-Bus handshake at "info": keep them for problems only.
    let default_filter = "info,zbus=warn,tracing=warn";
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_filter))
        .init();

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

    let mut handler = Handler::new(config, modules::Registry::start_all());
    let mut watcher = Watcher::new(&config_path);
    loop {
        let mut deck = wait_for_deck(&mut handler, &mut watcher);
        log::info!("ready, waiting for key presses");
        // Only returns on a device error, almost always because it was unplugged.
        let err = drive(&mut deck, &mut handler, &mut watcher).unwrap_err();
        log::warn!("Stream Deck lost: {err:#}");
        handler.release_all();
    }
}

/// Looks for the deck every `RETRY` until it is there. Modules keep ticking meanwhile.
fn wait_for_deck(handler: &mut Handler, watcher: &mut Watcher) -> Deck {
    let mut last_error = String::new();
    loop {
        match Deck::connect() {
            Ok(deck) => return deck,
            Err(err) => {
                // Log once per different reason, not every second.
                let error = format!("{err:#}");
                if error != last_error {
                    log::warn!("{error}; retrying every {}s", RETRY.as_secs());
                    last_error = error;
                }
            }
        }
        std::thread::sleep(RETRY);
        background(handler, watcher);
    }
}

/// Runs the deck until it fails. Never returns `Ok`.
fn drive(deck: &mut Deck, handler: &mut Handler, watcher: &mut Watcher) -> Result<()> {
    loop {
        deck.draw(&handler.screen(&clock(), Instant::now()))?;
        for event in deck.read_events(TICK)? {
            match event {
                DeviceStateUpdate::ButtonDown(key) => handler.key_down(key, Instant::now()),
                DeviceStateUpdate::ButtonUp(key) => handler.key_up(key),
                _ => {} // encoders/touch: not on the MK.2
            }
        }
        background(handler, watcher);
    }
}

/// Work done with or without a deck: module ticks and config reload.
fn background(handler: &mut Handler, watcher: &mut Watcher) {
    handler.tick(Instant::now());
    if watcher.changed() {
        // An invalid file keeps the current config running.
        match Config::load(&watcher.path) {
            Ok(config) => {
                handler.set_config(config);
                log::info!("config reloaded");
            }
            Err(err) => log::error!("config not reloaded: {err:#}"),
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
