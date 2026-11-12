//! `BetterStreamDux`: maps Stream Deck key presses to application modules.
//!
//! Flow: load config -> connect to deck -> draw keys -> wait for presses -> run module action.

mod config;
mod deck;
mod modules;

use anyhow::Result;
use elgato_streamdeck::DeviceStateUpdate;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());
    let config = config::Config::load(&config_path)?;
    log::info!("loaded {} buttons from {config_path}", config.buttons.len());

    let deck = deck::Deck::connect()?;
    deck.setup(&config)?;

    let mut modules = modules::Registry::new();

    log::info!("ready, waiting for key presses");
    loop {
        for event in deck.wait_for_events()? {
            // We only act when a key is pushed down, not when it is released.
            if let DeviceStateUpdate::ButtonDown(key) = event {
                on_key_press(&config, &mut modules, key);
            }
        }
    }
}

/// Runs the action bound to `key`. Errors are logged, they must never stop the program.
fn on_key_press(config: &config::Config, modules: &mut modules::Registry, key: u8) {
    let Some(button) = config.button(key) else {
        log::debug!("key {key} has no binding");
        return;
    };
    log::info!("key {key} -> {}::{}", button.module, button.action);
    if let Err(err) = modules.run(&button.module, &button.action) {
        log::error!("{}::{} failed: {err:#}", button.module, button.action);
    }
}
