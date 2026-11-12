//! `BetterStreamDux`: maps Stream Deck key presses to application modules.
//!
//! Loop: read key events -> handler runs actions -> build screen -> deck redraws what changed.

mod builtin;
mod config;
mod deck;
mod handler;
mod modules;
mod pages;
mod render;
mod screen;

use std::time::{Duration, Instant};

use anyhow::Result;
use elgato_streamdeck::DeviceStateUpdate;

/// How often the loop wakes up (long presses, clock), even with no key event.
const TICK: Duration = Duration::from_millis(50);

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());
    let config = config::Config::load(&config_path)?;
    log::info!(
        "loaded {} buttons on {} page(s) from {config_path}",
        config.buttons.len(),
        config.page_count()
    );

    let mut deck = deck::Deck::connect()?;
    let mut handler = handler::Handler::new(config, modules::Registry::new());

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
    }
}

/// Local time as "HH:MM", for `display = "clock"`.
fn clock() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}
