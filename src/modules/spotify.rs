//! Spotify control through MPRIS, the standard Linux D-Bus media interface.
//!
//! Actions: `play_pause`, `next`, `previous`, `stop`.

use anyhow::{Result, bail};
use zbus::blocking::Connection;

use super::Module;

const BUS_NAME: &str = "org.mpris.MediaPlayer2.spotify";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

pub struct Spotify;

impl Module for Spotify {
    fn name(&self) -> &'static str {
        "spotify"
    }

    fn run(&mut self, action: &str) -> Result<()> {
        let method = match action {
            "play_pause" => "PlayPause",
            "next" => "Next",
            "previous" => "Previous",
            "stop" => "Stop",
            other => bail!("unknown spotify action '{other}'"),
        };
        let bus = Connection::session()?;
        bus.call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), method, &())?;
        Ok(())
    }
}
