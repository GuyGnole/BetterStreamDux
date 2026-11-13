//! Spotify control and state through MPRIS, the standard Linux D-Bus media interface.
//! No account or API key needed.
//!
//! Actions: `play_pause`, `play`, `pause`, `next`, `previous`, `stop`,
//!          `volume_up`, `volume_down`, `seek_forward`, `seek_back` (10 s).
//! Values:  `status` (playing/paused/stopped), `title`, `artist`, `album`,
//!          `volume` (e.g. "60%"), `cover` (path to the downloaded album image).
//!          No values while Spotify is closed.
//!
//! Not possible here: shuffle and repeat. Spotify's Linux app accepts the MPRIS
//! Shuffle/LoopStatus properties but ignores them (tested). That needs the Spotify Web API.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use zbus::blocking::Connection;
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedValue;

use super::Module;
use crate::catalog::{Actions, ModuleInfo, action, image, state, text};
use crate::paths;

pub const INFO: ModuleInfo = ModuleInfo {
    name: "spotify",
    description: "Spotify playback, through MPRIS (no account needed)",
    actions: Actions::List(&[
        action("play_pause", "Play or pause"),
        action("play", "Play"),
        action("pause", "Pause"),
        action("next", "Next track"),
        action("previous", "Previous track"),
        action("stop", "Stop"),
        action("volume_up", "Spotify volume +10%"),
        action("volume_down", "Spotify volume -10%"),
        action("seek_forward", "Skip forward 10 s"),
        action("seek_back", "Skip back 10 s"),
    ]),
    values: &[
        state(
            "status",
            "Playback status",
            &["playing", "paused", "stopped"],
        ),
        text("title", "Track title", "Song title"),
        text("artist", "Artist(s)", "Artist"),
        text("album", "Album", "Album"),
        text("volume", "Spotify volume, like 60%", "60%"),
        image("cover", "Album cover image"),
    ],
};

/// How often the background thread reads Spotify's state.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const VOLUME_STEP: f64 = 0.1;
/// MPRIS seek offsets are in microseconds.
const SEEK_STEP_US: i64 = 10_000_000;

/// The MPRIS player interface. zbus generates `PlayerProxyBlocking` from it.
#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_service = "org.mpris.MediaPlayer2.spotify",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn play(&self) -> zbus::Result<()>;
    fn pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn stop(&self) -> zbus::Result<()>;
    fn seek(&self, offset_us: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    #[zbus(property)]
    fn volume(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn set_volume(&self, volume: f64) -> zbus::Result<()>;
}

pub struct Spotify {
    /// Latest values, written by the background thread, read by `value()`.
    values: Arc<Mutex<HashMap<String, String>>>,
}

impl Spotify {
    /// Creates the module and starts its background thread.
    pub fn start() -> Self {
        let values = Arc::new(Mutex::new(HashMap::new()));
        let shared = Arc::clone(&values);
        thread::spawn(move || watch(&shared));
        Self { values }
    }
}

impl Module for Spotify {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, action: &str) -> Result<()> {
        let player = connect()?;
        let result = match action {
            "play_pause" => player.play_pause(),
            "play" => player.play(),
            "pause" => player.pause(),
            "next" => player.next(),
            "previous" => player.previous(),
            "stop" => player.stop(),
            "volume_up" => player
                .volume()
                .and_then(|v| player.set_volume((v + VOLUME_STEP).min(1.0))),
            "volume_down" => player
                .volume()
                .and_then(|v| player.set_volume((v - VOLUME_STEP).max(0.0))),
            "seek_forward" => player.seek(SEEK_STEP_US),
            "seek_back" => player.seek(-SEEK_STEP_US),
            other => bail!("unknown spotify action '{other}'"),
        };
        result.context("is Spotify running?")
    }

    fn value(&self, name: &str) -> Option<String> {
        self.values.lock().ok()?.get(name).cloned()
    }
}

fn connect() -> Result<PlayerProxyBlocking<'static>> {
    let bus = Connection::session().context("cannot connect to D-Bus")?;
    // No property cache: always read fresh values from Spotify.
    let player = PlayerProxyBlocking::builder(&bus)
        .cache_properties(CacheProperties::No)
        .build()?;
    Ok(player)
}

/// Background thread: reads Spotify's state every `POLL_INTERVAL` into `shared`.
fn watch(shared: &Mutex<HashMap<String, String>>) {
    // Covers left by a previous run are stale.
    if let Ok(dir) = covers_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
    let mut player = None;
    let mut cover = Cover::default();
    loop {
        if player.is_none() {
            player = connect()
                .inspect_err(|err| log::warn!("spotify: {err:#}"))
                .ok();
        }
        // Spotify closed -> error -> no values, keys show their fallbacks.
        let values = player
            .as_ref()
            .and_then(|p| read_values(p, &mut cover).ok())
            .unwrap_or_default();
        if let Ok(mut shared) = shared.lock() {
            *shared = values;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn read_values(player: &PlayerProxyBlocking, cover: &mut Cover) -> Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    values.insert("status".into(), player.playback_status()?.to_lowercase());

    // Optional: if volume is missing, the other values still show.
    if let Ok(volume) = player.volume() {
        values.insert("volume".into(), format!("{:.0}%", volume * 100.0));
    }

    let metadata = player.metadata()?;
    let text = |key: &str| -> Option<String> {
        let value = metadata.get(key)?;
        <&str>::try_from(&**value).ok().map(str::to_string)
    };
    values.extend(text("xesam:title").map(|t| ("title".into(), t)));
    values.extend(text("xesam:album").map(|a| ("album".into(), a)));
    if let Some(artists) = metadata.get("xesam:artist") {
        let artists: Vec<String> = artists.try_clone()?.try_into()?;
        values.insert("artist".into(), artists.join(", "));
    }
    if let Some(path) = text("mpris:artUrl").and_then(|url| cover.path_for(&url)) {
        values.insert("cover".into(), path);
    }
    Ok(values)
}

/// Keeps the current album cover downloaded in the cache directory.
#[derive(Default)]
struct Cover {
    url: String,
    path: Option<String>,
}

impl Cover {
    /// Local path of the image at `url`, downloaded once per new url.
    fn path_for(&mut self, url: &str) -> Option<String> {
        if url != self.url {
            if let Some(old) = self.path.take() {
                let _ = std::fs::remove_file(old); // previous track's cover, not needed anymore
            }
            self.url = url.to_string();
            self.path = download(url)
                .inspect_err(|err| log::warn!("spotify cover: {err:#}"))
                .ok();
        }
        self.path.clone()
    }
}

fn download(url: &str) -> Result<String> {
    // Spotify urls look like https://i.scdn.co/image/<id>; the id becomes the file name.
    let id = url.rsplit('/').next().unwrap_or_default();
    ensure!(
        url.starts_with("https://")
            && !id.is_empty()
            && id.chars().all(|c| c.is_ascii_alphanumeric()),
        "unexpected cover url {url}"
    );
    let dir = covers_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join(format!("{id}.jpg"));

    let bytes = ureq::get(url).call()?.body_mut().read_to_vec()?;
    std::fs::write(&path, bytes).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

fn covers_dir() -> Result<std::path::PathBuf> {
    Ok(paths::cache_dir()?.join("spotify-covers"))
}
