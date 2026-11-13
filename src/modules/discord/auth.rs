//! Discord login: your app's credentials, and the OAuth token we keep and renew.
//!
//! Files (never in the git repo):
//!   `~/.config/betterstreamdux/discord.toml`             `client_id` + `client_secret`, written by you
//!   `~/.local/state/betterstreamdux/discord-token.json`  login token, written and renewed by us

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::paths;

const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
/// Must also be listed in your Discord app's `OAuth2` redirects (see README).
const REDIRECT_URI: &str = "http://localhost";
/// Renew the token when it has less than this left (seconds).
const RENEW_MARGIN_S: u64 = 24 * 3600;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}

impl Credentials {
    pub fn load() -> Result<Self> {
        let path = paths::config_dir()?.join("discord.toml");
        let text = fs::read_to_string(&path).with_context(|| {
            format!("cannot read {} (see README, Discord setup)", path.display())
        })?;
        toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
    }
}

// Field names follow Discord's OAuth vocabulary.
#[allow(clippy::struct_field_names)]
#[derive(Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    refresh_token: String,
    /// Unix time in seconds.
    expires_at: u64,
}

impl Token {
    /// The saved token, if any.
    pub fn load() -> Option<Self> {
        let text = fs::read_to_string(token_path().ok()?).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Saves the token, readable only by you (it gives access to your Discord voice).
    pub fn save(&self) -> Result<()> {
        paths::write_private(&token_path()?, &serde_json::to_string(self)?)
    }

    /// Forgets the saved token: next connection asks for approval again.
    pub fn delete() {
        if let Ok(path) = token_path() {
            let _ = fs::remove_file(path);
        }
    }

    pub fn expires_soon(&self) -> bool {
        self.expires_at < now() + RENEW_MARGIN_S
    }

    /// Turns the code from Discord's approval popup into a token.
    pub fn from_code(credentials: &Credentials, code: &str) -> Result<Self> {
        request_token(
            credentials,
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", REDIRECT_URI),
            ],
        )
    }

    /// Gets a fresh token without asking you again.
    pub fn renew(&self, credentials: &Credentials) -> Result<Self> {
        request_token(
            credentials,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &self.refresh_token),
            ],
        )
    }
}

fn request_token(credentials: &Credentials, params: &[(&str, &str)]) -> Result<Token> {
    #[derive(Deserialize)]
    struct Reply {
        access_token: String,
        refresh_token: String,
        expires_in: u64,
    }

    let mut form = vec![
        ("client_id", credentials.client_id.as_str()),
        ("client_secret", credentials.client_secret.as_str()),
    ];
    form.extend_from_slice(params);

    // Keep HTTP errors as normal replies so Discord's explanation ends up in our error message.
    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut response = agent.post(TOKEN_URL).send_form(form)?;
    let status = response.status();
    let body = response.body_mut().read_to_string()?;
    ensure!(
        status.is_success(),
        "Discord refused the login ({status}): {body}"
    );

    let reply: Reply = serde_json::from_str(&body).context("unexpected token reply")?;
    Ok(Token {
        access_token: reply.access_token,
        refresh_token: reply.refresh_token,
        expires_at: now() + reply.expires_in,
    })
}

fn token_path() -> Result<PathBuf> {
    Ok(paths::state_dir()?.join("discord-token.json"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
