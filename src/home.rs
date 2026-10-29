//! Where Karen lives: `~/.karen` on every platform, beside `~/.claude` and
//! `~/.codex`. `KAREN_HOME` moves the whole house.

use std::path::{Path, PathBuf};

/// The user's home: `HOME`, then `USERPROFILE`, then the platform's answer.
#[must_use]
pub fn user_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}

/// `KAREN_HOME`, else `~/.karen`.
#[must_use]
pub fn karen_home() -> PathBuf {
    if let Some(home) = std::env::var_os("KAREN_HOME") {
        return PathBuf::from(home);
    }
    user_home().map_or_else(|| PathBuf::from(".karen"), |h| h.join(".karen"))
}

/// `~/` expanded against the user's home; any other path unchanged.
#[must_use]
pub fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => user_home().map_or_else(|| PathBuf::from(path), |h| h.join(rest)),
        None => PathBuf::from(path),
    }
}

/// The user policy layer.
#[must_use]
pub fn user_policies() -> PathBuf {
    karen_home().join("policies")
}

/// Where derived data that can always be rebuilt is kept.
#[must_use]
pub fn cache_dir() -> PathBuf {
    karen_home().join("cache")
}

/// `path` relative to `base` when it is inside it, for short provenance.
#[must_use]
pub fn display_under(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}
