//! Everything that talks to the Stream Deck hardware lives here.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use elgato_streamdeck::{DeviceStateReader, DeviceStateUpdate, StreamDeck, info::Kind};

use crate::render;
use crate::screen::Screen;

pub struct Deck {
    device: Arc<StreamDeck>,
    reader: Arc<DeviceStateReader>,
    /// Last screen sent to the device, to redraw only what changed.
    shown: Option<Screen>,
}

impl Deck {
    /// Connects to the first Stream Deck MK.2 found.
    pub fn connect() -> Result<Self> {
        let hid = elgato_streamdeck::new_hidapi().context("cannot start hidapi")?;
        let (kind, serial) = elgato_streamdeck::list_devices(&hid)
            .into_iter()
            .find(|(kind, _)| *kind == Kind::Mk2)
            .context(
                "no Stream Deck MK.2 found (is it plugged in? are the udev rules installed?)",
            )?;
        let device = StreamDeck::connect(&hid, kind, &serial).context("cannot open Stream Deck")?;
        log::info!("connected to Stream Deck MK.2 (serial {serial})");
        // get_reader() needs an Arc. We stay single-threaded, so the Send/Sync lint does not apply.
        #[allow(clippy::arc_with_non_send_sync)]
        let device = Arc::new(device);
        let reader = device.get_reader();
        Ok(Self {
            device,
            reader,
            shown: None,
        })
    }

    /// Sends `screen` to the device. Only keys that changed since last time are redrawn.
    pub fn draw(&mut self, screen: &Screen) -> Result<()> {
        if self.shown.as_ref() == Some(screen) {
            return Ok(());
        }
        let old = self.shown.as_ref();
        if old.map(|s| s.brightness) != Some(screen.brightness) {
            self.device.set_brightness(screen.brightness)?;
        }
        for (key, face) in (0u8..).zip(&screen.keys) {
            if old.map(|s| &s.keys[usize::from(key)]) == Some(face) {
                continue;
            }
            // A broken icon must not stop the program: log it and leave the key black.
            match render::render(face) {
                Ok(image) => self.device.set_button_image(key, image)?,
                Err(err) => {
                    log::error!("key {key}: {err:#}");
                    self.device.clear_button_image(key)?;
                }
            }
        }
        self.device.flush()?;
        self.shown = Some(screen.clone());
        Ok(())
    }

    /// Waits up to `timeout` for key events (press, release...). Empty if nothing happened.
    pub fn read_events(&self, timeout: Duration) -> Result<Vec<DeviceStateUpdate>> {
        Ok(self.reader.read(Some(timeout))?)
    }
}
