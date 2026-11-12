//! Everything that talks to the Stream Deck hardware lives here.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use elgato_streamdeck::{DeviceStateReader, DeviceStateUpdate, StreamDeck, info::Kind};
use image::{DynamicImage, Rgb, RgbImage};

use crate::config::{Config, parse_color};

/// Pixel size of one key on the MK.2.
const KEY_SIZE: u32 = 72;

pub struct Deck {
    device: Arc<StreamDeck>,
    reader: Arc<DeviceStateReader>,
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
        Ok(Self { device, reader })
    }

    /// Applies brightness and draws every configured key.
    pub fn setup(&self, config: &Config) -> Result<()> {
        self.device.set_brightness(config.brightness)?;
        self.device.clear_all_button_images()?;
        for button in &config.buttons {
            let image = if let Some(path) = &button.icon {
                image::open(path).with_context(|| format!("cannot open icon {path}"))?
            } else if let Some(color) = &button.color {
                solid_color(parse_color(color)?)
            } else {
                continue;
            };
            self.device.set_button_image(button.key, image)?;
        }
        self.device.flush()?;
        Ok(())
    }

    /// Waits up to `timeout` for key events (press, release...). Empty if nothing happened.
    pub fn read_events(&self, timeout: Duration) -> Result<Vec<DeviceStateUpdate>> {
        Ok(self.reader.read(Some(timeout))?)
    }
}

fn solid_color(rgb: [u8; 3]) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_pixel(KEY_SIZE, KEY_SIZE, Rgb(rgb)))
}
