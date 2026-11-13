//! Spotify control and state through MPRIS, the standard Linux D-Bus media interface
//! (no account needed), plus Liked Songs through the Spotify Web API (login needed, see
//! README "Spotify setup"; `auth.rs` logs in, `web.rs` makes the calls).
//!
//! Actions: `play_pause`, `play`, `pause`, `next`, `previous`, `stop`,
//!          `volume_up`, `volume_down`, `seek_forward`, `seek_back` (10 s).
//!          Web API: `like`, `unlike`, `toggle_like` (current track in/out of Liked Songs),
//!          `dislike` (out of Liked Songs and skip: the API has no real "hide song").
//! Values:  `status` (playing/paused/stopped), `title`, `artist`, `album`,
//!          `volume` (e.g. "60%"), `cover` (path to the downloaded album image),
//!          `liked` (yes/no, once logged in). No values while Spotify is closed.
//!
//! Not possible here: shuffle and repeat. Spotify's Linux app accepts the MPRIS
//! Shuffle/LoopStatus properties but ignores them (tested). That needs the Spotify Web API.

mod auth;
mod web;

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use zbus::blocking::Connection;
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedValue;

use super::Module;
use crate::catalog::{Actions, ModuleInfo, action, image, preset, state, text};
use crate::paths;

pub const INFO: ModuleInfo = ModuleInfo {
    name: "spotify",
    description: "Spotify playback through MPRIS; likes through the Web API (login needed)",
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
        action("like", "Add the track to Liked Songs"),
        action("unlike", "Remove the track from Liked Songs"),
        action("toggle_like", "Like or unlike the track"),
        action("dislike", "Remove from Liked Songs and skip"),
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
        state("liked", "Track is in your Liked Songs", &["yes", "no"]),
    ],
    instances: false,
    presets: &[
        preset(
            "Play / Pause",
            r##"
symbol = "play"
short_press = "spotify:play_pause"
long_press = "spotify:stop"
color = "#535353"
look_by = { value = "spotify:status", states = { playing = { symbol = "pause", color = "#1DB954" } } }
"##,
        ),
        preset(
            "Next track",
            r##"
symbol = "next"
short_press = "spotify:next"
long_press = "spotify:seek_forward"
color = "#1DB954"
"##,
        ),
        preset(
            "Previous track",
            r##"
symbol = "previous"
short_press = "spotify:previous"
long_press = "spotify:seek_back"
color = "#1DB954"
"##,
        ),
        preset(
            "Like",
            r##"
symbol = "heart_outline"
short_press = "spotify:toggle_like"
color = "#191414"
look_by = { value = "spotify:liked", states = { yes = { symbol = "heart", symbol_color = "#1DB954" } } }
"##,
        ),
        preset(
            "Dislike",
            r##"
symbol = "thumbs_down"
short_press = "spotify:dislike"
color = "#7A1F1F"
"##,
        ),
        preset(
            "Album cover",
            r##"
icon_value = "spotify:cover"
short_press = "spotify:play_pause"
color = "#191414"
"##,
        ),
        preset(
            "Now playing",
            r##"
symbol = "music"
display = "spotify:title"
label = "Spotify"
color = "#191414"
"##,
        ),
        preset(
            "Volume +",
            r##"
symbol = "volume_up"
short_press = "spotify:volume_up"
color = "#333333"
"##,
        ),
        preset(
            "Volume -",
            r##"
symbol = "volume_down"
short_press = "spotify:volume_down"
color = "#333333"
"##,
        ),
        preset(
            "Open Spotify",
            r##"
symbol = "music"
label = "Spotify"
short_press = "shell:spotify"
color = "#1DB954"
"##,
        ),
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

/// State written by the background threads, read by `value()`.
#[derive(Default)]
struct Shared {
    /// Latest MPRIS values (watch thread).
    values: HashMap<String, String>,
    /// Current track, like "spotify:track:<id>". `None` for ads, local files, Spotify closed.
    uri: Option<String>,
    /// Last known Liked Songs answer (web thread): which track, and liked or not.
    liked: Option<(String, bool)>,
}

impl Shared {
    /// Liked or not, if known for the track playing now.
    fn liked_now(&self) -> Option<bool> {
        let (uri, liked) = self.liked.as_ref()?;
        (self.uri.as_ref() == Some(uri)).then_some(*liked)
    }
}

pub struct Spotify {
    shared: Arc<Mutex<Shared>>,
    /// Like/unlike requests for the web thread.
    web: Sender<web::Request>,
}

impl Spotify {
    /// Creates the module and starts its background threads.
    pub fn start() -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let watched = Arc::clone(&shared);
        thread::spawn(move || watch(&watched));
        let web = web::start(Arc::clone(&shared));
        Self { shared, web }
    }

    /// Like actions only send a request: the web thread does the slow HTTP call.
    fn run_like(&self, like: Like) -> Result<()> {
        let (uri, liked) = {
            let shared = self
                .shared
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock"))?;
            (shared.uri.clone(), shared.liked_now())
        };
        let uri = uri.context("no Spotify track playing")?;
        let liked = match like {
            Like::Add => true,
            Like::Remove | Like::Dislike => false,
            Like::Toggle => liked != Some(true),
        };
        self.web
            .send(web::Request { uri, liked })
            .context("Spotify web thread stopped")?;
        if like == Like::Dislike {
            connect()?.next().context("is Spotify running?")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Like {
    Add,
    Remove,
    Toggle,
    Dislike,
}

impl Like {
    fn parse(action: &str) -> Option<Self> {
        match action {
            "like" => Some(Self::Add),
            "unlike" => Some(Self::Remove),
            "toggle_like" => Some(Self::Toggle),
            "dislike" => Some(Self::Dislike),
            _ => None,
        }
    }
}

impl Module for Spotify {
    fn name(&self) -> &'static str {
        INFO.name
    }

    fn run(&mut self, _instance: &str, action: &str) -> Result<()> {
        if let Some(like) = Like::parse(action) {
            return self.run_like(like);
        }
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

    fn value(&self, _instance: &str, name: &str) -> Option<String> {
        let shared = self.shared.lock().ok()?;
        if name == "liked" {
            let liked = shared.liked_now()?;
            return Some(if liked { "yes" } else { "no" }.to_string());
        }
        shared.values.get(name).cloned()
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
fn watch(shared: &Mutex<Shared>) {
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
        let (values, uri) = player
            .as_ref()
            .and_then(|p| read_values(p, &mut cover).ok())
            .unwrap_or_default();
        if let Ok(mut shared) = shared.lock() {
            shared.values = values;
            shared.uri = uri;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// The values, and the current track's URI.
fn read_values(
    player: &PlayerProxyBlocking,
    cover: &mut Cover,
) -> Result<(HashMap<String, String>, Option<String>)> {
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
    let uri = text("xesam:url").and_then(|url| track_uri(&url));
    Ok((values, uri))
}

/// `https://open.spotify.com/track/<id>` -> `spotify:track:<id>` (episodes too).
/// `None` for anything else (ads, local files).
fn track_uri(url: &str) -> Option<String> {
    let (kind, id) = url
        .strip_prefix("https://open.spotify.com/")?
        .split_once('/')?;
    let valid = matches!(kind, "track" | "episode")
        && !id.is_empty()
        && id.chars().all(|c| c.is_ascii_alphanumeric());
    valid.then(|| format!("spotify:{kind}:{id}"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Actions;

    #[test]
    fn track_uri_from_url() {
        assert_eq!(
            track_uri("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC").as_deref(),
            Some("spotify:track:4uLU6hMCjMI75M1A2tKUQC")
        );
        assert_eq!(track_uri("https://open.spotify.com/ad/123"), None);
        assert_eq!(track_uri("file:///music/song.mp3"), None);
    }

    #[test]
    fn like_actions_are_in_the_catalog() {
        let Actions::List(actions) = INFO.actions else {
            panic!("spotify actions are a list");
        };
        let likes = actions.iter().filter(|a| Like::parse(a.name).is_some());
        assert_eq!(likes.count(), 4);
    }

    #[test]
    fn liked_only_for_the_current_track() {
        let mut shared = Shared {
            uri: Some("spotify:track:a".into()),
            liked: Some(("spotify:track:a".into(), true)),
            ..Shared::default()
        };
        assert_eq!(shared.liked_now(), Some(true));
        shared.uri = Some("spotify:track:b".into());
        assert_eq!(shared.liked_now(), None, "next track: not known yet");
    }
}
