//! Per-user directories for module files (XDG standard).

use std::path::PathBuf;

use anyhow::{Context, Result};

/// `~/.cache/betterstreamdux`: files that can be deleted anytime (album covers...).
pub fn cache_dir() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".cache"),
    };
    Ok(base.join("betterstreamdux"))
}
