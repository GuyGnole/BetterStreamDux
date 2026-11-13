//! Per-user directories for module files (XDG standard).

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// `~/.config/betterstreamdux`: files you write (e.g. Discord app credentials).
pub fn config_dir() -> Result<PathBuf> {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// `~/.local/state/betterstreamdux`: files the program keeps between runs (login tokens).
pub fn state_dir() -> Result<PathBuf> {
    xdg_dir("XDG_STATE_HOME", ".local/state")
}

/// `~/.cache/betterstreamdux`: files that can be deleted anytime (album covers...).
pub fn cache_dir() -> Result<PathBuf> {
    xdg_dir("XDG_CACHE_HOME", ".cache")
}

/// Writes a file only you can read (login tokens), creating its directory if needed.
pub fn write_private(path: &Path, contents: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("cannot write {}", path.display()))
}

/// `$VAR/betterstreamdux`, or `$HOME/<fallback>/betterstreamdux` when `$VAR` is unset.
fn xdg_dir(var: &str, fallback: &str) -> Result<PathBuf> {
    let base = match std::env::var_os(var) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(home()?).join(fallback),
    };
    Ok(base.join("betterstreamdux"))
}

fn home() -> Result<OsString> {
    std::env::var_os("HOME").context("HOME is not set")
}
