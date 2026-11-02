//! Where Karen lives: `~/.karen` on every platform, beside `~/.claude` and
//! `~/.codex`. `KAREN_HOME` moves the whole house. The runtimes she guards move
//! theirs too, and she follows: `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and
//! `OPENCODE_CONFIG_DIR`.

use std::path::PathBuf;

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

/// A directory named by `var`, unless the variable is unset or empty.
fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Claude Code's configuration directory: `CLAUDE_CONFIG_DIR`, else `~/.claude`.
/// Its settings, plugins and transcripts live here.
#[must_use]
pub fn claude_config_dir() -> Option<PathBuf> {
    env_dir("CLAUDE_CONFIG_DIR").or_else(|| user_home().map(|h| h.join(".claude")))
}

/// Claude Code's user state file, which holds its user MCP servers: inside
/// `CLAUDE_CONFIG_DIR` when that is set, else `~/.claude.json`.
#[must_use]
pub fn claude_json() -> Option<PathBuf> {
    env_dir("CLAUDE_CONFIG_DIR")
        .map(|d| d.join(".claude.json"))
        .or_else(|| user_home().map(|h| h.join(".claude.json")))
}

/// Codex's home: `CODEX_HOME`, else `~/.codex`. Its `config.toml`, hooks and
/// session transcripts live here.
#[must_use]
pub fn codex_home() -> Option<PathBuf> {
    env_dir("CODEX_HOME").or_else(|| user_home().map(|h| h.join(".codex")))
}

/// `OpenCode`'s global configuration directory: `$XDG_CONFIG_HOME/opencode`,
/// else `~/.config/opencode`.
#[must_use]
pub fn opencode_global_dir() -> Option<PathBuf> {
    env_dir("XDG_CONFIG_HOME")
        .or_else(|| user_home().map(|h| h.join(".config")))
        .map(|d| d.join("opencode"))
}

/// Where `OpenCode` reads plugins and configuration besides the project:
/// `OPENCODE_CONFIG_DIR` when set, else its global directory.
#[must_use]
pub fn opencode_config_dir() -> Option<PathBuf> {
    env_dir("OPENCODE_CONFIG_DIR").or_else(opencode_global_dir)
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
