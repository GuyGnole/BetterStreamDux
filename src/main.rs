//! `BetterStreamDux`: maps Stream Deck key presses to application modules.
//!
//! Flow: load config -> connect to deck -> draw keys -> loop: read key events -> handler.

mod config;
mod deck;
mod handler;
mod modules;

use std::time::{Duration, Instant};

use anyhow::Result;
use elgato_streamdeck::DeviceStateUpdate;

/// How often the loop wakes up to check for long presses, even with no key event.
const TICK: Duration = Duration::from_millis(50);

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());
    let config = config::Config::load(&config_path)?;
    log::info!("loaded {} buttons from {config_path}", config.buttons.len());

    let deck = deck::Deck::connect()?;
    deck.setup(&config)?;

    let mut handler = handler::Handler::new(config, modules::Registry::new());

    log::info!("ready, waiting for key presses");
    loop {
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
