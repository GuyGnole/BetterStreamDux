//! Discord control. NOT IMPLEMENTED YET.
//!
//! Planned actions: `toggle_mute`, `toggle_deafen`.
//!
//! Possible approaches (to decide):
//!   - Discord RPC over its local IPC socket (`$XDG_RUNTIME_DIR/discord-ipc-0`).
//!     Voice control needs the `rpc.voice.write` OAuth scope, which requires a
//!     registered Discord application.
//!   - Simulate the Discord keybind with `ydotool` (works on Wayland, needs the
//!     `ydotoold` daemon running).

use anyhow::{Result, bail};

use super::Module;

pub struct Discord;

impl Module for Discord {
    fn name(&self) -> &'static str {
        "discord"
    }

    fn run(&mut self, action: &str) -> Result<()> {
        bail!("discord module not implemented yet (action '{action}')")
    }
}
