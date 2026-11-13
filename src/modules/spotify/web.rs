//! Spotify Web API thread: adds/removes the current track from your Liked Songs, and keeps
//! `Shared::liked` up to date. Runs in its own thread because HTTP calls are slow.
//!
//! Uses the library endpoints (`/me/library`, with track URIs), the ones still open to
//! personal "Development Mode" apps since Spotify's February 2026 API changes.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};

use super::{Shared, auth};

const LIBRARY_URL: &str = "https://api.spotify.com/v1/me/library";
/// How often the thread looks for a new track (it then asks if it is liked).
const LOOK_INTERVAL: Duration = Duration::from_secs(1);
/// Asks again this often for the same track, in case you liked it in the Spotify app.
const RECHECK: Duration = Duration::from_secs(30);

/// Put `uri` in your Liked Songs (`liked`) or take it out.
pub struct Request {
    pub uri: String,
    pub liked: bool,
}

/// Starts the thread. Send it `Request`s; it writes results into `shared.liked`.
pub fn start(shared: Arc<Mutex<Shared>>) -> Sender<Request> {
    let (sender, requests) = mpsc::channel();
    thread::spawn(move || serve(&requests, &shared));
    sender
}

fn serve(requests: &Receiver<Request>, shared: &Mutex<Shared>) {
    // Last track asked about, and when: one check per new track, then every RECHECK.
    let mut checked: Option<(String, Instant)> = None;
    loop {
        match requests.recv_timeout(LOOK_INTERVAL) {
            Ok(request) => {
                if let Err(err) = set_liked(&request, shared) {
                    log::error!("spotify like: {err:#}");
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return, // module dropped
        }
        let Some(uri) = shared.lock().ok().and_then(|s| s.uri.clone()) else {
            continue;
        };
        let due = checked
            .as_ref()
            .is_none_or(|(last, at)| *last != uri || at.elapsed() >= RECHECK);
        if due {
            // Remembered even on failure, so a problem is logged every RECHECK, not every second.
            checked = Some((uri.clone(), Instant::now()));
            if let Err(err) = check(&uri, shared) {
                log::warn!("spotify like status: {err:#}");
            }
        }
    }
}

fn set_liked(request: &Request, shared: &Mutex<Shared>) -> Result<()> {
    let token = auth::access_token(true)?.context("not logged in to Spotify")?;
    let agent = agent();
    let response = if request.liked {
        put_uri(agent.put(LIBRARY_URL), &request.uri, &token).send_empty()
    } else {
        put_uri(agent.delete(LIBRARY_URL), &request.uri, &token).call()
    };
    read(response)?;
    log::info!(
        "spotify: {} {}",
        if request.liked { "liked" } else { "unliked" },
        request.uri
    );
    store(shared, &request.uri, request.liked);
    Ok(())
}

/// Asks Spotify if `uri` is liked. Does nothing until you have logged in once.
fn check(uri: &str, shared: &Mutex<Shared>) -> Result<()> {
    let Some(token) = auth::access_token(false)? else {
        return Ok(());
    };
    let request = agent().get(format!("{LIBRARY_URL}/contains"));
    let body = read(put_uri(request, uri, &token).call())?;
    // One boolean per URI asked.
    let answer: Vec<bool> = serde_json::from_str(&body).context("unexpected Spotify reply")?;
    store(shared, uri, *answer.first().context("empty Spotify reply")?);
    Ok(())
}

fn store(shared: &Mutex<Shared>, uri: &str, liked: bool) {
    if let Ok(mut shared) = shared.lock() {
        shared.liked = Some((uri.to_string(), liked));
    }
}

/// Adds the track and the login to a request.
fn put_uri<B>(request: ureq::RequestBuilder<B>, uri: &str, token: &str) -> ureq::RequestBuilder<B> {
    request
        .query("uris", uri)
        .header("Authorization", format!("Bearer {token}"))
}

/// HTTP errors stay normal replies, so Spotify's explanation ends up in our error message.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent()
}

/// The reply's body, or an error with Spotify's explanation.
fn read(response: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<String> {
    let mut response = response.context("cannot reach Spotify")?;
    let status = response.status();
    let body = response.body_mut().read_to_string()?;
    ensure!(status.is_success(), "Spotify answered {status}: {body}");
    Ok(body)
}
