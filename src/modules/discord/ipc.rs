//! Discord's local RPC socket: finding it, and reading/writing its frames.
//!
//! A frame is: opcode (u32 little-endian) + payload length (u32 little-endian) + JSON payload.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
const OP_CLOSE: u32 = 2;
const OP_PING: u32 = 3;
const OP_PONG: u32 = 4;

/// How long `read` waits for a new frame. Also the max delay before a key press is sent.
const POLL: Duration = Duration::from_millis(20);
/// Once a frame has started, the rest must arrive within this time.
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Ipc {
    stream: UnixStream,
}

impl Ipc {
    /// Connects to the first Discord socket found (native, Flatpak or Snap install).
    pub fn connect() -> Result<Self> {
        for path in socket_paths()? {
            if let Ok(stream) = UnixStream::connect(&path) {
                log::debug!("discord: connected to {}", path.display());
                return Ok(Self { stream });
            }
        }
        bail!("Discord is not running (no discord-ipc socket found)")
    }

    pub fn handshake(&mut self, client_id: &str) -> Result<()> {
        self.write(OP_HANDSHAKE, &json!({ "v": 1, "client_id": client_id }))
    }

    pub fn send(&mut self, payload: &Value) -> Result<()> {
        self.write(OP_FRAME, payload)
    }

    /// Waits up to `POLL` for a message. `None` if nothing arrived.
    pub fn read(&mut self) -> Result<Option<Value>> {
        let mut header = [0u8; 8];
        self.stream.set_read_timeout(Some(POLL))?;
        match self.stream.read(&mut header[..1]) {
            Ok(0) => bail!("Discord closed the connection"),
            Ok(_) => {}
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        }
        // A frame has started: read the rest with a longer timeout so it is never cut in half.
        self.stream.set_read_timeout(Some(FRAME_TIMEOUT))?;
        self.stream.read_exact(&mut header[1..])?;
        let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let mut payload = vec![0; usize::try_from(length)?];
        self.stream.read_exact(&mut payload)?;
        let payload: Value =
            serde_json::from_slice(&payload).context("invalid JSON from Discord")?;

        match opcode {
            OP_FRAME => Ok(Some(payload)),
            OP_PING => {
                self.write(OP_PONG, &payload)?;
                Ok(None)
            }
            OP_CLOSE => bail!("Discord closed the connection: {}", payload["message"]),
            other => bail!("unexpected Discord opcode {other}"),
        }
    }

    fn write(&mut self, opcode: u32, payload: &Value) -> Result<()> {
        let body = serde_json::to_vec(payload)?;
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend(opcode.to_le_bytes());
        frame.extend(u32::try_from(body.len())?.to_le_bytes());
        frame.extend(body);
        self.stream.write_all(&frame)?;
        Ok(())
    }
}

/// `discord-ipc-0` to `-9`, in each place Discord may put it.
fn socket_paths() -> Result<Vec<PathBuf>> {
    let runtime =
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?);
    let dirs = [
        runtime.clone(),                            // native package
        runtime.join("app/com.discordapp.Discord"), // Flatpak
        runtime.join("snap.discord"),               // Snap
    ];
    Ok(dirs
        .iter()
        .flat_map(|dir| (0..10).map(move |n| dir.join(format!("discord-ipc-{n}"))))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `Ipc` talking to a fake Discord (the other end of a socket pair).
    fn pair() -> (Ipc, Ipc) {
        let (a, b) = UnixStream::pair().unwrap();
        (Ipc { stream: a }, Ipc { stream: b })
    }

    #[test]
    fn frames_round_trip() {
        let (mut us, mut discord) = pair();
        us.send(&json!({ "cmd": "GET_VOICE_SETTINGS" })).unwrap();
        let received = discord.read().unwrap().unwrap();
        assert_eq!(received["cmd"], "GET_VOICE_SETTINGS");
    }

    #[test]
    fn nothing_to_read_is_none() {
        let (mut us, _discord) = pair();
        assert!(us.read().unwrap().is_none());
    }

    #[test]
    fn ping_is_answered_with_pong() {
        let (mut us, mut discord) = pair();
        discord.write(OP_PING, &json!({ "n": 1 })).unwrap();
        assert!(us.read().unwrap().is_none(), "ping is not a message");
        // Read the raw pong header on Discord's side.
        let mut header = [0u8; 4];
        discord.stream.read_exact(&mut header).unwrap();
        assert_eq!(u32::from_le_bytes(header), OP_PONG);
    }

    #[test]
    fn close_is_an_error() {
        let (mut us, mut discord) = pair();
        discord
            .write(OP_CLOSE, &json!({ "message": "bye" }))
            .unwrap();
        assert!(us.read().is_err());
    }
}
