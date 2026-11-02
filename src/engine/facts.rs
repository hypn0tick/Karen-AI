//! Environment facts: what the runtime around her actually has, so a gate can say
//! "if this session has the Brave MCP server" instead of someone remembering it does.
//! A gate reads them as `facts.<name>`; she collects only the facts that a gate which
//! applies to this event names, and a fact the runtime already supplied wins.
//! The Karen who checks what's in the building before quoting the fire code.

use crate::policy::PolicySet;
use crate::state::StateStore;
use crate::types::EngineRequest;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Facts stable for a session, cached in the state store under `facts.<name>`.
const SESSION_STABLE: &[&str] = &["runtime", "mcp_servers", "tools"];
/// Most uncommitted paths reported, so a large dirty tree stays a small payload.
const GIT_LINES: usize = 50;

/// Adds every referenced fact to `req.raw["facts"]`.
pub fn enrich(set: &PolicySet, state: &dyn StateStore, req: &mut EngineRequest) {
    let wanted: BTreeSet<String> = set
        .gates
        .iter()
        .filter(|g| g.applies_to(&req.event, req.tool_name.as_deref()))
        .flat_map(|g| g.matcher.fields())
        .filter_map(|f| f.strip_prefix("facts."))
        .map(|f| f.split('.').next().unwrap_or(f).to_string())
        .collect();
    if wanted.is_empty() {
        return;
    }
    let mut facts: Map<String, Value> = req
        .raw
        .get("facts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let runtime = runtime(&req.raw);
    for name in wanted {
        if facts.contains_key(&name) {
            continue;
        }
        let cache_key = format!("facts.{name}");
        let stable = SESSION_STABLE.contains(&name.as_str());
        if stable {
            if let Some(cached) = state.get(&req.session_id, &cache_key) {
                facts.insert(name, cached);
                continue;
            }
        }
        let value = match name.as_str() {
            "runtime" => Value::from(runtime.clone()),
            "mcp_servers" => Value::from(join(mcp_servers(&runtime, req.cwd.as_deref()))),
            "tools" => Value::from(join(tools_on_path(&set.config.facts.tools))),
            "git_uncommitted" => Value::from(git_uncommitted(req.cwd.as_deref())),
            _ => continue,
        };
        if stable {
            state.set(&req.session_id, &cache_key, value.clone());
        }
        facts.insert(name, value);
    }
    if let Value::Object(raw) = &mut req.raw {
        raw.insert("facts".into(), Value::Object(facts));
    }
}

fn join(names: BTreeSet<String>) -> String {
    names.into_iter().collect::<Vec<_>>().join(",")
}

/// The runtime a payload came from: the `facts.runtime` its bridge supplied
/// (the `OpenCode` plugin says `opencode`), else [`detect_runtime`].
#[must_use]
pub fn runtime(raw: &Value) -> String {
    raw.get("facts")
        .and_then(|f| f.get("runtime"))
        .and_then(Value::as_str)
        .map_or_else(|| detect_runtime(raw), str::to_string)
}

/// The runtime, from what its payload carries: Claude Code and Codex name their
/// transcript under their own configuration directories, wherever
/// `CLAUDE_CONFIG_DIR` and `CODEX_HOME` put them, else under a `.claude` or
/// `.codex` directory. The hook commands `karen setup` installs name the
/// runtime outright (`--runtime`), so this is the fallback for hand-written
/// hooks.
fn detect_runtime(raw: &Value) -> String {
    let transcript = norm(
        raw.get("transcript_path")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let under = |dir: Option<PathBuf>| {
        dir.is_some_and(|d| transcript.starts_with(&format!("{}/", norm(&d.to_string_lossy()))))
    };
    if under(crate::home::codex_home()) {
        "codex".into()
    } else if under(crate::home::claude_config_dir()) {
        "claude".into()
    } else if transcript.contains("/.codex/") {
        "codex".into()
    } else if transcript.contains("/.claude/") {
        "claude".into()
    } else {
        "unknown".into()
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    json5::from_str(&text).ok()
}

/// Object keys whose value does not say `enabled: false`. Names only: server
/// definitions can carry keys and tokens, and a gate never needs them.
fn enabled_keys(obj: Option<&Value>, out: &mut BTreeSet<String>) {
    let Some(map) = obj.and_then(Value::as_object) else {
        return;
    };
    for (name, def) in map {
        if def.get("enabled").and_then(Value::as_bool) != Some(false) {
            out.insert(name.clone());
        }
    }
}

/// A path compared by text: `/`-separated, no trailing `/`, lower case.
fn norm(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn same_path(a: &str, b: &str) -> bool {
    norm(a) == norm(b)
}

/// Enabled MCP server names the runtime would load for this working directory,
/// read from the runtime's own configuration files.
fn mcp_servers(runtime: &str, cwd: Option<&str>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    match runtime {
        "claude" => {
            if let Some(user) = crate::home::claude_json().and_then(|p| read_json(&p)) {
                enabled_keys(user.get("mcpServers"), &mut names);
                if let (Some(cwd), Some(projects)) =
                    (cwd, user.get("projects").and_then(Value::as_object))
                {
                    for (dir, project) in projects {
                        if same_path(dir, cwd) {
                            enabled_keys(project.get("mcpServers"), &mut names);
                        }
                    }
                }
            }
            if let Some(cwd) = cwd {
                if let Some(project) = read_json(&Path::new(cwd).join(".mcp.json")) {
                    enabled_keys(project.get("mcpServers"), &mut names);
                }
            }
            for plugin in crate::home::claude_config_dir()
                .map(|dir| enabled_claude_plugins(&dir))
                .unwrap_or_default()
            {
                if let Some(mcp) = read_json(&plugin.join(".mcp.json")) {
                    let servers = mcp.get("mcpServers").unwrap_or(&mcp);
                    enabled_keys(Some(servers), &mut names);
                }
            }
        }
        "codex" => {
            let text = crate::home::codex_home()
                .and_then(|dir| std::fs::read_to_string(dir.join("config.toml")).ok())
                .unwrap_or_default();
            if let Ok(toml::Value::Table(table)) = text.parse::<toml::Value>() {
                if let Some(toml::Value::Table(servers)) = table.get("mcp_servers") {
                    for (name, def) in servers {
                        if def.get("enabled").and_then(toml::Value::as_bool) != Some(false) {
                            names.insert(name.clone());
                        }
                    }
                }
            }
        }
        "opencode" => {
            // Each directory holds `opencode.json` or `opencode.jsonc`.
            let config_files = |dir: &Path| {
                ["opencode.json", "opencode.jsonc"]
                    .into_iter()
                    .map(|name| dir.join(name))
                    .collect::<Vec<_>>()
            };
            let mut files = crate::home::opencode_global_dir()
                .map(|dir| config_files(&dir))
                .unwrap_or_default();
            if let Some(cwd) = cwd {
                files.extend(config_files(Path::new(cwd)));
            }
            if let Some(custom) = std::env::var_os("OPENCODE_CONFIG").filter(|v| !v.is_empty()) {
                files.push(PathBuf::from(custom));
            }
            if let Some(dir) = std::env::var_os("OPENCODE_CONFIG_DIR").filter(|v| !v.is_empty()) {
                files.extend(config_files(Path::new(&dir)));
            }
            // Later files override earlier ones, as OpenCode merges them.
            let mut merged: Map<String, Value> = Map::new();
            for file in files {
                if let Some(Value::Object(mcp)) =
                    read_json(&file).and_then(|c| c.get("mcp").cloned())
                {
                    merged.extend(mcp);
                }
            }
            enabled_keys(Some(&Value::Object(merged)), &mut names);
        }
        _ => {}
    }
    names
}

/// Install directories of the plugins Claude Code's user settings enable, in its
/// configuration directory `config`.
fn enabled_claude_plugins(config: &Path) -> Vec<PathBuf> {
    let Some(settings) = read_json(&config.join("settings.json")) else {
        return Vec::new();
    };
    let Some(enabled) = settings.get("enabledPlugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    let cache = config.join("plugins").join("cache");
    enabled
        .iter()
        .filter(|(_, on)| on.as_bool() == Some(true))
        .filter_map(|(id, _)| {
            let (name, market) = id.split_once('@')?;
            let versions = std::fs::read_dir(cache.join(market).join(name)).ok()?;
            versions
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .max()
        })
        .collect()
}

/// The configured tool names found on `PATH`.
fn tools_on_path(wanted: &[String]) -> BTreeSet<String> {
    let dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let suffixes: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };
    wanted
        .iter()
        .filter(|tool| {
            dirs.iter().any(|d| {
                suffixes
                    .iter()
                    .any(|s| d.join(format!("{tool}{s}")).is_file())
            })
        })
        .cloned()
        .collect()
}

/// `git status --porcelain` for the working directory, one path per line; empty
/// outside a repository.
fn git_uncommitted(cwd: Option<&str>) -> String {
    let Some(cwd) = cwd else {
        return String::new();
    };
    let Ok(out) = Command::new("git")
        .args(["-C", cwd, "status", "--porcelain"])
        .output()
    else {
        return String::new();
    };
    if !out.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .take(GIT_LINES)
        .collect::<Vec<_>>()
        .join("\n")
}
