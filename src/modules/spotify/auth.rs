//! Spotify login for the Web API: your app's credentials, and the token we keep and renew.
//!
//! The first time a key needs it, your browser opens Spotify's approval page; Spotify then
//! sends the browser back to a tiny local web server here (`REDIRECT_URI`) with a code.
//!
//! Files (never in the git repo):
//!   `~/.config/betterstreamdux/spotify.toml`             `client_id` + `client_secret`, written by you
//!   `~/.local/state/betterstreamdux/spotify-token.json`  login token, written and renewed by us

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::paths;

const AUTHORIZE_URL: &str = "https://accounts.spotify.com/authorize";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
/// Must also be listed in your Spotify app's Redirect URIs (see README).
/// Spotify refuses "localhost": the IP must be written out.
pub const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const LISTEN_ADDRESS: &str = "127.0.0.1:8888";
const SCOPES: &str = "user-library-read user-library-modify";
/// How long we wait for you to approve in the browser.
const LOGIN_TIMEOUT: Duration = Duration::from_mins(5);
/// Renew the token when it has less than this left (seconds). Tokens last one hour.
const RENEW_MARGIN_S: u64 = 60;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    client_id: String,
    client_secret: String,
}

impl Credentials {
    fn load() -> Result<Self> {
        let path = paths::config_dir()?.join("spotify.toml");
        let text = fs::read_to_string(&path).with_context(|| {
            format!("cannot read {} (see README, Spotify setup)", path.display())
        })?;
        toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
    }
}

// Field names follow the OAuth vocabulary.
#[allow(clippy::struct_field_names)]
#[derive(Serialize, Deserialize)]
struct Token {
    access_token: String,
    refresh_token: String,
    /// Unix time in seconds.
    expires_at: u64,
}

/// A valid access token. Renews the saved one if needed. Without a saved login:
/// opens the browser to log in if `may_login`, otherwise `None` (background checks
/// must not pop up a browser on their own).
pub fn access_token(may_login: bool) -> Result<Option<String>> {
    let token = match load_token() {
        Some(token) if token.expires_at > now() + RENEW_MARGIN_S => {
            return Ok(Some(token.access_token));
        }
        Some(token) => renew(&Credentials::load()?, &token.refresh_token).inspect_err(|_| {
            // Renewal refused (e.g. access removed): the next key press logs in again.
            let _ = token_path().map(fs::remove_file);
        })?,
        None if may_login => login(&Credentials::load()?)?,
        None => return Ok(None),
    };
    paths::write_private(&token_path()?, &serde_json::to_string(&token)?)?;
    Ok(Some(token.access_token))
}

fn load_token() -> Option<Token> {
    let text = fs::read_to_string(token_path().ok()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn token_path() -> Result<PathBuf> {
    Ok(paths::state_dir()?.join("spotify-token.json"))
}

/// Opens Spotify's approval page in the browser and waits for its answer.
fn login(credentials: &Credentials) -> Result<Token> {
    let state = random_hex()?; // proves the answer comes from our request
    let listener = TcpListener::bind(LISTEN_ADDRESS)
        .with_context(|| format!("cannot listen on {LISTEN_ADDRESS} for the Spotify login"))?;
    let url = format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={state}",
        url_encode(&credentials.client_id),
        url_encode(REDIRECT_URI),
        url_encode(SCOPES),
    );
    log::info!("spotify: approve in your browser (or open {url})");
    if let Err(err) = Command::new("xdg-open").arg(&url).status() {
        log::warn!("spotify: cannot open the browser: {err}");
    }
    let code = wait_for_code(&listener, &state)?;
    log::info!("spotify: logged in");
    request_token(
        credentials,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT_URI),
        ],
        None,
    )
}

fn renew(credentials: &Credentials, refresh_token: &str) -> Result<Token> {
    request_token(
        credentials,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
        Some(refresh_token),
    )
}

/// `old_refresh`: kept when Spotify's reply has no new refresh token (renewals often don't).
fn request_token(
    credentials: &Credentials,
    params: &[(&str, &str)],
    old_refresh: Option<&str>,
) -> Result<Token> {
    #[derive(Deserialize)]
    struct Reply {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: u64,
    }

    let secret = format!("{}:{}", credentials.client_id, credentials.client_secret);
    // Keep HTTP errors as normal replies so Spotify's explanation ends up in our error message.
    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut response = agent
        .post(TOKEN_URL)
        .header(
            "Authorization",
            format!("Basic {}", base64(secret.as_bytes())),
        )
        .send_form(params.to_vec())?;
    let status = response.status();
    let body = response.body_mut().read_to_string()?;
    ensure!(
        status.is_success(),
        "Spotify refused the login ({status}): {body}"
    );

    let reply: Reply = serde_json::from_str(&body).context("unexpected token reply")?;
    Ok(Token {
        access_token: reply.access_token,
        refresh_token: reply
            .refresh_token
            .or(old_refresh.map(str::to_string))
            .context("no refresh token in Spotify's reply")?,
        expires_at: now() + reply.expires_in,
    })
}

/// Answers browser requests until one brings the login code, or `LOGIN_TIMEOUT`.
fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    // Non-blocking so we can give up after the timeout instead of waiting forever.
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(code) = answer(stream, state)? {
                    return Ok(code);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(200));
            }
            Err(err) => return Err(err).context("Spotify login: local server failed"),
        }
    }
    bail!(
        "Spotify login not approved within {} minutes",
        LOGIN_TIMEOUT.as_secs() / 60
    )
}

/// Reads one browser request and replies with a short page.
/// `Some(code)` for Spotify's redirect, `None` for anything else (e.g. the favicon).
fn answer(mut stream: TcpStream, state: &str) -> Result<Option<String>> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let result = parse_callback(&line, state);
    let (status, message) = match &result {
        Ok(Some(_)) => (
            "200 OK",
            "Spotify login done, you can close this tab.".to_string(),
        ),
        Ok(None) => ("404 Not Found", "Not found.".to_string()),
        Err(err) => ("400 Bad Request", format!("Spotify login failed: {err:#}")),
    };
    // Best effort: the code is all we need, even if the page does not reach the browser.
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{message}"
    );
    result
}

/// Reads the first line of the browser's request, like
/// `GET /callback?code=XYZ&state=ABC HTTP/1.1`.
fn parse_callback(request_line: &str, state: &str) -> Result<Option<String>> {
    let target = request_line.split(' ').nth(1).unwrap_or_default();
    let Some(query) = target.strip_prefix("/callback?") else {
        return Ok(None);
    };
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
    };
    if let Some(error) = param("error") {
        bail!("refused in the browser ({error})");
    }
    ensure!(param("state") == Some(state), "unexpected login answer");
    let code = param("code").context("no code in Spotify's answer")?;
    Ok(Some(code.to_string()))
}

/// 16 random bytes as hex, from the kernel.
fn random_hex() -> Result<String> {
    let mut bytes = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .context("cannot read /dev/urandom")?;
    Ok(bytes.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}"); // writing to a String cannot fail
        hex
    }))
}

/// Percent-encodes everything but letters, digits and `-_.~` (for URL parameters).
fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Standard base64 with padding, for the HTTP Basic header (avoids a dependency).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"id:secret"), "aWQ6c2VjcmV0");
    }

    #[test]
    fn url_encode_keeps_safe_chars() {
        assert_eq!(
            url_encode("http://127.0.0.1:8888/callback"),
            "http%3A%2F%2F127.0.0.1%3A8888%2Fcallback"
        );
        assert_eq!(url_encode("a b-c_d.e~"), "a%20b-c_d.e~");
    }

    #[test]
    fn callback_gives_code_or_error() {
        let ok = "GET /callback?code=abc-123&state=s1 HTTP/1.1\r\n";
        assert_eq!(
            parse_callback(ok, "s1").unwrap().as_deref(),
            Some("abc-123")
        );
        assert!(parse_callback(ok, "other").is_err(), "wrong state");
        let refused = "GET /callback?error=access_denied&state=s1 HTTP/1.1";
        assert!(parse_callback(refused, "s1").is_err());
        let favicon = "GET /favicon.ico HTTP/1.1";
        assert_eq!(parse_callback(favicon, "s1").unwrap(), None);
    }

    #[test]
    fn random_hex_is_32_chars() {
        let a = random_hex().unwrap();
        assert_eq!(a.len(), 32);
        assert_ne!(a, random_hex().unwrap());
    }
}
