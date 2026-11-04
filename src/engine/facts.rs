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

/// Every fact name a gate may read under `facts.`: the list `docs/session-facts.md`
/// answers to. `env` stands for `facts.env.<NAME>`, one allowlisted variable each.
pub const FACT_NAMES: &[&str] = &[
    "runtime",
    "mcp_servers",
    "tools",
    "git_uncommitted",
    "session_id",
    "context_tokens",
    "context_window",
    "context_ratio",
    "provider_error",
    "env",
];

/// Facts stable for a session, cached in the state store under `facts.<name>`.
const SESSION_STABLE: &[&str] = &["runtime", "mcp_servers", "tools"];
/// Most uncommitted paths reported, so a large dirty tree stays a small payload.
const GIT_LINES: usize = 50;
/// Where a transcript tail read starts: the occupancy of the latest turn sits
/// in its last lines, so one read covers the common case.
const TAIL_START: u64 = 256 * 1024;
/// The most a tail read grows to before she calls the fact unreadable: the
/// last usage line can sit behind megabytes of tool output, but never behind
/// unbounded megabytes. Past the cap the fact is absent, never guessed.
const TAIL_MAX: u64 = 8 * 1024 * 1024;

/// The facts the gates that apply to this event name, by the path after
/// `facts.` (`env.<NAME>` names the one variable it reads; the single-segment
/// facts match their own arms in [`enrich`]). A gate names a fact by matching
/// on it, by latching on it, or by templating it into a reason or an argument:
/// the latch watches a field of the payload too, and a templated fact the
/// gate does not collect renders `(unknown)`. A templated `facts.env.*` is
/// never rendered, so it collects nothing.
fn wanted_facts(set: &PolicySet, req: &EngineRequest) -> BTreeSet<String> {
    let mut wanted: BTreeSet<String> = set
        .gates
        .iter()
        .filter(|g| g.applies_to(&req.event, req.tool_name.as_deref()))
        .flat_map(|g| {
            let named = g
                .matcher
                .fields()
                .chain(g.state.as_ref().and_then(|s| s.field.as_deref()))
                .filter_map(|f| f.strip_prefix("facts."));
            let templated = g
                .action
                .templated_facts()
                .into_iter()
                .filter(|path| !path.starts_with("env."));
            named
                .chain(templated)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    // The ratio is read from the two facts it divides: a gate that names it
    // asks for both, whichever of the three it matches on.
    if wanted.contains("context_ratio") {
        wanted.insert("context_tokens".into());
        wanted.insert("context_window".into());
    }
    wanted
}

/// Adds every referenced fact to `req.raw["facts"]`.
pub fn enrich(set: &PolicySet, state: &dyn StateStore, req: &mut EngineRequest) {
    let wanted = wanted_facts(set, req);
    if wanted.is_empty() {
        return;
    }
    let mut facts: Map<String, Value> = req
        .raw
        .get("facts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // `env` is never a payload fact: the allowlist over her own environment is
    // its only source, so nothing a bridge hands her can reach a gate as a
    // variable the user never named.
    facts.remove("env");
    let runtime = runtime(&req.raw);
    let env = env_facts(set, &wanted);
    if !env.is_empty() {
        facts.insert("env".into(), Value::Object(env));
    }
    // The occupancy of the latest turn, read once for both facts that share
    // it. Only Claude Code and Codex record it in a transcript she can read; a
    // runtime that supplies its own facts (the OpenCode bridge) wins without
    // her reading anything, so the facts a gate names stay absent here.
    let want_tokens = wanted.contains("context_tokens") && !facts.contains_key("context_tokens");
    let want_window = wanted.contains("context_window") && !facts.contains_key("context_window");
    let latest_turn: Option<(u64, Option<u64>)> =
        if (want_tokens || want_window) && matches!(runtime.as_str(), "claude" | "codex") {
            transcript_path(&req.raw).and_then(|path| match runtime.as_str() {
                "claude" => read_tail(Path::new(path), TAIL_START, TAIL_MAX, claude_usage)
                    .map(|tokens| (tokens, None)),
                _ => read_tail(Path::new(path), TAIL_START, TAIL_MAX, codex_usage),
            })
        } else {
            None
        };
    let want_ratio = wanted.contains("context_ratio");
    // `facts.provider_error`: the provider's own last failure, whatever it said
    // it broke on. Read only on the events a failure rides in with a transcript
    // (Stop, ProviderError); a fact the runtime supplied (the OpenCode bridge)
    // wins without her reading anything.
    let want_error = wanted
        .iter()
        .any(|f| f == "provider_error" || f.starts_with("provider_error."))
        && !facts.contains_key("provider_error");
    if want_error {
        if let Some(error) = transcript_error(&runtime, &req.raw) {
            facts.insert("provider_error".into(), error);
        }
    }
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
        // A fact she cannot read is absent, never zero: no occupancy fact
        // invents a number, so a gate matching on one never matches on noise.
        let value: Option<Value> = match name.as_str() {
            "runtime" => Some(Value::from(runtime.clone())),
            "mcp_servers" => Some(Value::from(join(mcp_servers(&runtime, req.cwd.as_deref())))),
            "tools" => Some(Value::from(join(tools_on_path(&set.config.facts.tools)))),
            "git_uncommitted" => git_uncommitted(req.cwd.as_deref()).map(Value::from),
            // The session's own id, as the runtime said it: the payload's word,
            // absent when the payload carries none.
            "session_id" => req
                .raw
                .get("session_id")
                .and_then(Value::as_str)
                .map(Value::from),
            "context_tokens" => latest_turn.map(|(tokens, _)| Value::from(tokens)),
            "context_window" => latest_turn
                .and_then(|(_, window)| window)
                .or_else(|| configured_window(set, &runtime))
                .map(Value::from),
            _ => continue,
        };
        let Some(value) = value else {
            continue;
        };
        if stable {
            state.set(&req.session_id, &cache_key, value.clone());
        }
        facts.insert(name, value);
    }
    if want_ratio && !facts.contains_key("context_ratio") {
        if let Some(ratio) = context_ratio(&facts) {
            facts.insert("context_ratio".into(), Value::from(ratio));
        }
    }
    if let Value::Object(raw) = &mut req.raw {
        raw.insert("facts".into(), Value::Object(facts));
    }
}

fn join(names: BTreeSet<String>) -> String {
    names.into_iter().collect::<Vec<_>>().join(",")
}

/// The context window configured for a runtime (`[facts.context_window]`
/// `claude = ...`), when the runtime does not report its own. The key names the
/// runtime, so one runtime's number never answers for another's.
fn configured_window(set: &PolicySet, runtime: &str) -> Option<u64> {
    set.config
        .facts
        .context_window
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(runtime))
        .map(|(_, window)| *window)
}

/// `facts.env`: her own environment, and only the names the user layer
/// allowlisted. A name not on the list is never read — not an error, not a
/// warning, just a fact that stays absent. Unset or not UTF-8 is absent too;
/// an empty value is a value, and reads as the empty string.
fn env_facts(set: &PolicySet, wanted: &BTreeSet<String>) -> Map<String, Value> {
    let mut env = Map::new();
    for name in wanted.iter().filter_map(|f| f.strip_prefix("env.")) {
        if !set.config.facts.env.iter().any(|allowed| allowed == name) {
            continue;
        }
        if let Ok(value) = std::env::var(name) {
            env.insert(name.to_string(), Value::from(value));
        }
    }
    env
}

/// `facts.context_ratio`: the latest turn's occupancy over the window it runs
/// in, absent when either side is missing or the window is zero, because a
/// ratio of nothing over nothing is noise, not a number.
fn context_ratio(facts: &Map<String, Value>) -> Option<f64> {
    let tokens = facts.get("context_tokens")?.as_f64()?;
    let window = facts.get("context_window")?.as_f64()?;
    (window > 0.0).then_some(tokens / window)
}

/// Where the runtime recorded this session's turns, when it says so.
fn transcript_path(raw: &Value) -> Option<&str> {
    raw.get("transcript_path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
}

/// The provider's own last failure, from the transcript its runtime records —
/// Claude Code's API-error entry, Codex's `error` event — absent when the
/// runtime records none, or when a later assistant entry got through.
fn transcript_error(runtime: &str, raw: &Value) -> Option<Value> {
    if !matches!(runtime, "claude" | "codex") {
        return None;
    }
    let path = transcript_path(raw)?;
    let read = read_tail(
        Path::new(path),
        TAIL_START,
        TAIL_MAX,
        |line| match runtime {
            "claude" => claude_error(line),
            _ => codex_error(line),
        },
    );
    read.flatten()
}

/// The last line `parse` accepts, reading the file's tail backwards in windows
/// that double from `start` to `max`: the line she wants sits at a turn's end,
/// but a turn's last lines can be megabytes of tool output, so a fixed tail can
/// stop short of it. A rejected line costs one parse; past `max` the answer is
/// none, so a fact she cannot read is absent, never guessed.
fn read_tail<T>(path: &Path, start: u64, max: u64, parse: impl Fn(&str) -> Option<T>) -> Option<T> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut window = start.min(max);
    loop {
        let begin = len.saturating_sub(window);
        file.seek(SeekFrom::Start(begin)).ok()?;
        let mut bytes = Vec::new();
        (&mut file).take(len - begin).read_to_end(&mut bytes).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines().rev() {
            if let Some(found) = parse(line) {
                return Some(found);
            }
        }
        if begin == 0 || window >= max {
            return None;
        }
        window = window.saturating_mul(2).min(max);
    }
}

/// The occupancy of Claude Code's latest assistant turn: `input_tokens` plus
/// both cache counts, from the last entry of its session transcript that
/// carries them.
fn claude_usage(line: &str) -> Option<u64> {
    let entry: Value = serde_json::from_str(line).ok()?;
    if entry.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let usage = entry.get("message")?.get("usage")?;
    if !usage.is_object() {
        return None;
    }
    let count = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    Some(
        count("input_tokens")
            + count("cache_read_input_tokens")
            + count("cache_creation_input_tokens"),
    )
}

/// The occupancy of Codex's latest turn and the window it runs in, from the
/// last `token_count` event of its rollout: `last_token_usage` and
/// `model_context_window`. A closed rollout is zstd-compressed and she does not
/// decompress it, so unreadable stays absent.
fn codex_usage(line: &str) -> Option<(u64, Option<u64>)> {
    let entry: Value = serde_json::from_str(line).ok()?;
    if entry.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = entry.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return None;
    }
    let info = payload.get("info")?;
    let usage = info.get("last_token_usage")?;
    if !usage.is_object() {
        return None;
    }
    let count = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    let tokens =
        count("input_tokens") + count("cached_input_tokens") + count("cache_write_input_tokens");
    Some((
        tokens,
        info.get("model_context_window").and_then(Value::as_u64),
    ))
}

/// One line of a transcript, as the provider's last word on its own health:
/// `Some(Some(fact))` — this line failed, and here is what it said; `Some(None)`
/// — this line is a later success, and the failure before it is stale;
/// `None` — not a line that decides, keep reading backwards.
type LastWord = Option<Option<Value>>;

/// Claude Code's own apology, from the last assistant entry of its transcript:
/// an `isApiErrorMessage` entry carries the provider's words in its content; a
/// plain assistant entry after it means the call got through and the apology
/// is stale. The entry's model travels with the message, as the entry carries it.
fn claude_error(line: &str) -> LastWord {
    let Ok(entry) = serde_json::from_str::<Value>(line) else {
        return None;
    };
    if entry.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    if entry.get("isApiErrorMessage").and_then(Value::as_bool) != Some(true) {
        return Some(None);
    }
    let message = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .and_then(|parts| {
            parts
                .iter()
                .find_map(|part| part.get("text").and_then(Value::as_str))
        });
    match message {
        Some(message) => Some(Some(serde_json::json!({
            "message": message,
            "model": entry.get("message").and_then(|m| m.get("model")),
        }))),
        // An error entry whose words she cannot read: absent, never guessed.
        None => Some(None),
    }
}

/// Codex's last word, from its rollout (`EventMsg::Error` behind
/// `RolloutItem::EventMsg`, `tag = "type"`, `content = "payload"` — shape
/// verified against the codex-rs source, `codex-rs/protocol/src/protocol.rs`):
/// an `error` event carries its message; an assistant response item after it
/// means the call got through.
fn codex_error(line: &str) -> LastWord {
    let Ok(entry) = serde_json::from_str::<Value>(line) else {
        return None;
    };
    let payload = entry.get("payload")?;
    match payload.get("type").and_then(Value::as_str) {
        Some("error") => {
            let message = payload.get("message").and_then(Value::as_str);
            message.map(|m| Some(serde_json::json!({ "message": m })))
        }
        Some("message") if payload.get("role").and_then(Value::as_str) == Some("assistant") => {
            Some(None)
        }
        _ => None,
    }
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

/// `git status --porcelain` for the working directory, one path per line. A
/// clean repository reads as the empty string; no working directory, no
/// repository there, or a git that will not answer is absent, because an
/// empty string would claim a clean tree she never looked at.
fn git_uncommitted(cwd: Option<&str>) -> Option<String> {
    let out = crate::proc::command("git")
        .args(["-C", cwd?, "status", "--porcelain"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .take(GIT_LINES)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_uncommitted_is_absent_without_a_directory_or_a_repository() {
        assert_eq!(git_uncommitted(None), None);
        let nowhere = std::env::temp_dir().join("karen-no-such-directory-for-git");
        assert_eq!(git_uncommitted(nowhere.to_str()), None);
    }

    fn occupancy(tokens: u64, window: u64) -> Map<String, Value> {
        let mut facts = Map::new();
        facts.insert("context_tokens".into(), Value::from(tokens));
        facts.insert("context_window".into(), Value::from(window));
        facts
    }

    /// The facts the reference page's first column names, as `facts.<name>` with
    /// any `.<NAME>` tail cut off.
    fn documented_facts() -> Vec<String> {
        include_str!("../../docs/session-facts.md")
            .lines()
            .skip_while(|line| *line != "## Where each fact comes from")
            .skip(1)
            .take_while(|line| !line.starts_with("## "))
            .filter_map(|line| line.strip_prefix("| `facts."))
            .filter_map(|row| row.split('`').next())
            .filter_map(|name| name.split('.').next())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_reference_page_documents_exactly_the_facts_she_collects() {
        // A page that names a fact she does not collect promises a gate that
        // can never fire; a fact missing from it is one nobody can find.
        let mut documented = documented_facts();
        documented.sort();
        let mut known: Vec<String> = FACT_NAMES.iter().map(|name| (*name).to_string()).collect();
        known.sort();
        assert_eq!(documented, known);
    }

    #[test]
    fn ratio_divides_tokens_by_the_window() {
        assert_eq!(context_ratio(&occupancy(500, 1000)), Some(0.5));
    }

    #[test]
    fn ratio_is_absent_over_a_zero_window() {
        // Dividing by nothing gives infinity or NaN, and serde_json writes both
        // as null, so only the function itself can tell a real ratio from noise:
        // a window of zero is a window she does not know.
        assert_eq!(context_ratio(&occupancy(500, 0)), None);
        assert_eq!(context_ratio(&occupancy(0, 0)), None);
    }

    #[test]
    fn ratio_is_absent_when_either_side_is() {
        let mut facts = occupancy(500, 1000);
        facts.remove("context_window");
        assert_eq!(context_ratio(&facts), None);
        let mut facts = occupancy(500, 1000);
        facts.remove("context_tokens");
        assert_eq!(context_ratio(&facts), None);
    }

    #[test]
    fn tail_read_grows_its_window_until_the_line_fits() {
        // The Karen who reads backwards before she reads more: the line she
        // wants sits further back than her first window, and she doubles the
        // window instead of calling the fact unreadable.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tail.txt");
        let text = format!("{}\nfound\n{}", "x".repeat(300), "y".repeat(300));
        std::fs::write(&path, text).expect("write fixture");
        let found = read_tail(&path, 64, 1024, |line| {
            (line == "found").then_some(line.to_string())
        });
        assert_eq!(found.as_deref(), Some("found"));
    }

    #[test]
    fn tail_read_stops_at_the_cap_and_reports_nothing() {
        // Past the cap the answer is none: a fact she cannot read is absent,
        // never a guess dressed up as a number.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cap.txt");
        let text = format!("found\n{}", "y".repeat(300));
        std::fs::write(&path, text).expect("write fixture");
        let found: Option<String> = read_tail(&path, 64, 128, |line| {
            (line == "found").then(|| line.to_string())
        });
        assert_eq!(found, None);
    }

    #[test]
    fn tail_read_covers_a_whole_small_file_in_one_window() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("small.txt");
        std::fs::write(&path, "noise\nfound\n").expect("write fixture");
        let found = read_tail(&path, 64, 128, |line| {
            (line == "found").then(|| line.to_string())
        });
        assert_eq!(found.as_deref(), Some("found"));
    }

    #[test]
    fn claude_usage_sums_input_and_both_cache_counts() {
        let line = r#"{"type":"assistant","message":{"model":"m","usage":{"input_tokens":200,"cache_read_input_tokens":4000,"cache_creation_input_tokens":136,"output_tokens":50}}}"#;
        assert_eq!(claude_usage(line), Some(4336));
    }

    #[test]
    fn claude_usage_skips_lines_without_usage() {
        // A user line or a broken entry is skipped, not summed as zero.
        assert_eq!(claude_usage(r#"{"type":"user","message":{}}"#), None);
        assert_eq!(claude_usage(r#"{"type":"assistant"}"#), None);
        assert_eq!(
            claude_usage(r#"{"type":"assistant","message":{"usage":"nope"}}"#),
            None
        );
        assert_eq!(claude_usage("not json"), None);
    }

    #[test]
    fn codex_usage_reads_the_last_token_count() {
        let line = r#"{"timestamp":"t","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{},"last_token_usage":{"input_tokens":100,"cached_input_tokens":200,"cache_write_input_tokens":300,"output_tokens":1},"model_context_window":258400}}}"#;
        assert_eq!(codex_usage(line), Some((600, Some(258_400))));
    }

    #[test]
    fn codex_usage_reports_the_window_absent_rather_than_none_of_it() {
        // No `model_context_window` in the event: the tokens still count, and
        // the window is absent, so a configured window can answer for it.
        let line = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":7}}}}"#;
        assert_eq!(codex_usage(line), Some((7, None)));
    }

    #[test]
    fn codex_usage_skips_other_events() {
        assert_eq!(
            codex_usage(r#"{"type":"event_msg","payload":{"type":"agent_message"}}"#),
            None
        );
        assert_eq!(codex_usage(r#"{"type":"turn_context"}"#), None);
    }
}
