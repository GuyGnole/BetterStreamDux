//! Discord control and state through Discord's local RPC socket.
//! Needs a one-time setup: your own Discord app (see README, Discord setup).
//!
//! Actions: `mute`, `unmute`, `toggle_mute`, `deafen`, `undeafen`, `toggle_deafen`,
//!          `join_channel ID`, `leave_channel`.
//! Values:  `status` (connected/authorizing/disconnected), `muted` (true/false),
//!          `deafened` (true/false), `channel` (voice channel name, none when not in voice).
//!
//! A background thread owns the socket: it keeps the values up to date from Discord's
//! events, and sends the actions it receives from `run()` through a channel.

mod auth;
mod ipc;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use self::auth::{Credentials, Token};
use self::ipc::Ipc;
use super::Module;
use crate::catalog::{Actions, Arg, ModuleInfo, action, action_with, preset, state, text};

pub const INFO: ModuleInfo = ModuleInfo {
    name: "discord",
    description: "Discord voice, through Discord's local RPC (one-time setup, see README)",
    actions: Actions::List(&[
        action("mute", "Mute your microphone"),
        action("unmute", "Unmute your microphone"),
        action("toggle_mute", "Mute / unmute"),
        action("deafen", "Deafen (mute what you hear)"),
        action("undeafen", "Undeafen"),
        action("toggle_deafen", "Deafen / undeafen"),
        action_with("join_channel", Arg::Id, "Join a voice channel by id"),
        action("leave_channel", "Leave the voice channel"),
    ]),
    values: &[
        state(
            "status",
            "Connection to Discord",
            &["connected", "authorizing", "disconnected"],
        ),
        state("muted", "Microphone muted", &["true", "false"]),
        state("deafened", "Deafened", &["true", "false"]),
        text("channel", "Current voice channel name", "General"),
    ],
    instances: false,
    presets: &[
        preset(
            "Mute",
            r##"
symbol = "mic"
short_press = "discord:toggle_mute"
color = "#5865F2"
look_by = { value = "discord:muted", states = { true = { symbol = "mute", color = "#ED4245" } } }
"##,
        ),
        preset(
            "Deafen",
            r##"
symbol = "headphones"
short_press = "discord:toggle_deafen"
color = "#5865F2"
look_by = { value = "discord:deafened", states = { true = { symbol = "mute", color = "#ED4245" } } }
"##,
        ),
        preset(
            "Push to talk",
            r##"
symbol = "studio_mic"
label = "PTT"
while_press = { press = "discord:unmute", release = "discord:mute" }
color = "#5865F2"
look_by = { value = "discord:muted", states = { false = { color = "#3BA55D" } } }
"##,
        ),
        preset(
            "Voice channel",
            r##"
symbol = "people"
display = "discord:channel"
label = "No voice"
long_press = "discord:leave_channel"
color = "#2B2D31"
"##,
        ),
        preset(
            "Discord status",
            r##"
symbol = "chat"
display = "discord:status"
color = "#2B2D31"
"##,
        ),
    ],
};

type Values = Mutex<HashMap<String, String>>;

/// Wait between connection attempts (Discord closed, not set up...).
const RETRY: Duration = Duration::from_secs(5);
/// Time to click "Authorize" in Discord's popup.
const AUTHORIZE_TIMEOUT: Duration = Duration::from_mins(5);
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
const SCOPES: [&str; 3] = ["rpc", "rpc.voice.read", "rpc.voice.write"];

pub struct Discord {
    commands: Sender<Command>,
    values: Arc<Values>,
}

impl Discord {
    /// Creates the module and starts its background thread.
    pub fn start() -> Self {
        let (commands, receiver) = mpsc::channel();
        let values = Arc::new(Mutex::new(HashMap::new()));
        let shared = Arc::clone(&values);
        thread::spawn(move || worker(&receiver, &shared));
        Self { commands, values }
    }
}

impl Module for Discord {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, _instance: &str, action: &str) -> Result<()> {
        let command = Command::parse(action)?;
        let status = self.value("", "status").unwrap_or_default();
        ensure!(status == "connected", "Discord is not connected ({status})");
        self.commands
            .send(command)
            .context("discord thread stopped")
    }

    fn value(&self, _instance: &str, name: &str) -> Option<String> {
        self.values.lock().ok()?.get(name).cloned()
    }
}

#[derive(Debug, PartialEq)]
enum Command {
    Mute(Switch),
    Deafen(Switch),
    JoinChannel(String),
    LeaveChannel,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Switch {
    On,
    Off,
    Toggle,
}

impl Switch {
    fn apply(self, current: bool) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Toggle => !current,
        }
    }
}

impl Command {
    fn parse(action: &str) -> Result<Self> {
        Ok(match action.split_once(' ') {
            None => match action {
                "mute" => Self::Mute(Switch::On),
                "unmute" => Self::Mute(Switch::Off),
                "toggle_mute" => Self::Mute(Switch::Toggle),
                "deafen" => Self::Deafen(Switch::On),
                "undeafen" => Self::Deafen(Switch::Off),
                "toggle_deafen" => Self::Deafen(Switch::Toggle),
                "leave_channel" => Self::LeaveChannel,
                _ => bail!("unknown discord action '{action}'"),
            },
            Some(("join_channel", id)) if id.trim().chars().all(|c| c.is_ascii_digit()) => {
                Self::JoinChannel(id.trim().to_string())
            }
            Some(_) => bail!("unknown discord action '{action}'"),
        })
    }
}

/// Background thread: connects, runs a session until it fails, waits, tries again.
fn worker(commands: &Receiver<Command>, values: &Values) {
    let mut last_error = String::new();
    loop {
        let error = match session(commands, values) {
            Ok(()) => continue,
            Err(error) => format!("{error:#}"),
        };
        if let Ok(mut values) = values.lock() {
            values.clear();
            values.insert("status".into(), "disconnected".into());
        }
        // Log each new problem once, not every retry.
        if error != last_error {
            log::warn!("discord: {error}");
            last_error = error;
        }
        thread::sleep(RETRY);
    }
}

/// One connection to Discord. Only returns on error (Discord closed...).
fn session(commands: &Receiver<Command>, values: &Values) -> Result<()> {
    let credentials = Credentials::load()?;
    let mut s = Session {
        ipc: Ipc::connect()?,
        values,
        nonce: 0,
    };
    s.ipc.handshake(&credentials.client_id)?;
    s.wait_for(REPLY_TIMEOUT, |frame| frame["evt"] == "READY")?;
    s.login(&credentials)?;

    s.request(
        "SUBSCRIBE",
        json!({}),
        Some("VOICE_SETTINGS_UPDATE"),
        REPLY_TIMEOUT,
    )?;
    s.request(
        "SUBSCRIBE",
        json!({}),
        Some("VOICE_CHANNEL_SELECT"),
        REPLY_TIMEOUT,
    )?;
    s.send("GET_VOICE_SETTINGS", json!({}))?;
    s.send("GET_SELECTED_VOICE_CHANNEL", json!({}))?;

    while commands.try_recv().is_ok() {} // drop presses made while disconnected
    s.set("status", "connected");
    log::info!("discord: connected");

    loop {
        if let Some(frame) = s.ipc.read()? {
            s.handle(&frame)?;
        }
        while let Ok(command) = commands.try_recv() {
            s.execute(command)?;
        }
    }
}

struct Session<'a> {
    ipc: Ipc,
    values: &'a Values,
    /// Numbers each request, to match it with Discord's reply.
    nonce: u64,
}

impl Session<'_> {
    /// Authenticates with the saved token, renewing it or asking for approval when needed.
    fn login(&mut self, credentials: &Credentials) -> Result<()> {
        let token = match Token::load() {
            Some(token) if !token.expires_soon() => token,
            Some(token) => match token.renew(credentials) {
                Ok(token) => token,
                Err(err) => {
                    log::warn!("discord: token renewal failed ({err:#}), asking again");
                    self.authorize(credentials)?
                }
            },
            None => self.authorize(credentials)?,
        };
        token.save()?;
        let args = json!({ "access_token": token.access_token });
        if let Err(err) = self.request("AUTHENTICATE", args, None, REPLY_TIMEOUT) {
            Token::delete(); // rejected: ask for approval again on the next connection
            return Err(err.context("Discord rejected the saved login"));
        }
        Ok(())
    }

    /// Shows Discord's approval popup and waits for you to click "Authorize".
    fn authorize(&mut self, credentials: &Credentials) -> Result<Token> {
        self.set("status", "authorizing");
        log::info!("discord: click \"Authorize\" in the Discord window");
        let args = json!({ "client_id": credentials.client_id, "scopes": SCOPES });
        let reply = self.request("AUTHORIZE", args, None, AUTHORIZE_TIMEOUT)?;
        let code = reply["data"]["code"]
            .as_str()
            .context("no code in Discord's reply")?;
        Token::from_code(credentials, code)
    }

    /// Updates the values from a Discord reply or event.
    fn handle(&mut self, frame: &Value) -> Result<()> {
        let cmd = frame["cmd"].as_str().unwrap_or_default();
        let evt = frame["evt"].as_str().unwrap_or_default();
        let data = &frame["data"];
        if evt == "ERROR" {
            log::error!("discord {cmd}: {}", data["message"]);
            return Ok(());
        }
        match (cmd, evt) {
            ("GET_VOICE_SETTINGS" | "SET_VOICE_SETTINGS", _)
            | ("DISPATCH", "VOICE_SETTINGS_UPDATE") => {
                self.set_bool("muted", &data["mute"]);
                self.set_bool("deafened", &data["deaf"]);
            }
            // The event only has the channel id: ask for the full channel to get its name.
            ("DISPATCH", "VOICE_CHANNEL_SELECT") => {
                self.send("GET_SELECTED_VOICE_CHANNEL", json!({}))?;
            }
            ("GET_SELECTED_VOICE_CHANNEL" | "SELECT_VOICE_CHANNEL", _) => {
                match data["name"].as_str() {
                    Some(name) => self.set("channel", name),
                    None => self.remove("channel"), // not in a voice channel
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn execute(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Mute(switch) => {
                let mute = switch.apply(self.get_bool("muted"));
                self.send("SET_VOICE_SETTINGS", json!({ "mute": mute }))
            }
            Command::Deafen(switch) => {
                let deaf = switch.apply(self.get_bool("deafened"));
                self.send("SET_VOICE_SETTINGS", json!({ "deaf": deaf }))
            }
            Command::JoinChannel(id) => self.send(
                "SELECT_VOICE_CHANNEL",
                json!({ "channel_id": id, "force": true }),
            ),
            Command::LeaveChannel => {
                self.send("SELECT_VOICE_CHANNEL", json!({ "channel_id": null }))
            }
        }
    }

    /// Sends a command; its reply arrives later through `handle`.
    fn send(&mut self, cmd: &str, args: Value) -> Result<()> {
        self.send_with_evt(cmd, args, None).map(|_| ())
    }

    /// Sends a command and waits for its reply. Discord errors become `Err`.
    fn request(
        &mut self,
        cmd: &str,
        args: Value,
        evt: Option<&str>,
        timeout: Duration,
    ) -> Result<Value> {
        let nonce = self.send_with_evt(cmd, args, evt)?;
        let reply = self.wait_for(timeout, |frame| frame["nonce"] == nonce)?;
        if reply["evt"] == "ERROR" {
            bail!("{cmd} failed: {}", reply["data"]["message"]);
        }
        Ok(reply)
    }

    /// Returns the nonce used.
    fn send_with_evt(&mut self, cmd: &str, args: Value, evt: Option<&str>) -> Result<String> {
        self.nonce += 1;
        let nonce = self.nonce.to_string();
        let mut payload = json!({ "cmd": cmd, "nonce": nonce });
        payload["args"] = args;
        if let Some(evt) = evt {
            payload["evt"] = json!(evt);
        }
        self.ipc.send(&payload)?;
        Ok(nonce)
    }

    /// Reads messages until one matches. Others are ignored (only used during setup).
    fn wait_for(&mut self, timeout: Duration, matches: impl Fn(&Value) -> bool) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(frame) = self.ipc.read()?
                && matches(&frame)
            {
                return Ok(frame);
            }
        }
        bail!("Discord did not answer in time")
    }

    fn set(&self, name: &str, value: &str) {
        if let Ok(mut values) = self.values.lock() {
            values.insert(name.into(), value.into());
        }
    }

    fn remove(&self, name: &str) {
        if let Ok(mut values) = self.values.lock() {
            values.remove(name);
        }
    }

    fn set_bool(&self, name: &str, value: &Value) {
        if let Some(value) = value.as_bool() {
            self.set(name, if value { "true" } else { "false" });
        }
    }

    fn get_bool(&self, name: &str) -> bool {
        self.values
            .lock()
            .is_ok_and(|values| values.get(name).is_some_and(|v| v == "true"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_actions() {
        assert_eq!(
            Command::parse("toggle_mute").unwrap(),
            Command::Mute(Switch::Toggle)
        );
        assert_eq!(
            Command::parse("undeafen").unwrap(),
            Command::Deafen(Switch::Off)
        );
        assert_eq!(
            Command::parse("join_channel 123456").unwrap(),
            Command::JoinChannel("123456".into())
        );
        assert!(Command::parse("join_channel general").is_err());
        assert!(Command::parse("fly").is_err());
    }

    #[test]
    fn every_catalog_action_parses() {
        let Actions::List(actions) = INFO.actions else {
            panic!("discord actions are a list");
        };
        for info in actions {
            let arg = (info.arg != Arg::None).then_some("123");
            let action = crate::catalog::join_action(info.name, arg);
            assert!(Command::parse(&action).is_ok(), "{action}");
        }
    }

    #[test]
    fn switch_applies() {
        assert!(Switch::On.apply(false));
        assert!(!Switch::Off.apply(true));
        assert!(Switch::Toggle.apply(false));
    }
}
