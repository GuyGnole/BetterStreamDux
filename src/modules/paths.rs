//! Per-user directories for module files (XDG standard).

use std::ffi::OsString;
use std::path::PathBuf;

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
