// tests/e2e/runtime_integration/opencode_commands.rs
// The Karen who tests the OpenCode front desk: a command handed back by her
// binary is typed through the server's own API when the session is idle, and
// every outcome is written down. The plugin runs for real under node; the
// client it talks to is a mock that records what it was asked.

use super::opencode::install_bridge;
use crate::e2e::common::commands::{command_gate, runtime_lines, runtime_log, user_layer};
use crate::e2e::common::{DEFAULT_CONFIG, TestEnv, runtime_available};
use serde_json::Value;
use serial_test::serial;

/// The plugin under a mock client. `__SUMMARIZE__` is the client's
/// `session.summarize`; `__PLUGIN__` and `__DIR__` are filled in.
const PRELUDE: &str = r#"
const { KarenAI } = await import("__PLUGIN__");
const calls = { summarize: [], command: [], tui: [] };
const nap = (ms) => new Promise((r) => setTimeout(r, ms));
const client = {
  session: {
    get: async () => ({ data: __SESSION__ }),
    messages: async () => ({ data: [{ info: { id: "a1", role: "assistant" }, parts: [{ type: "text", text: "done" }] }] }),
    todo: async () => ({ data: [] }),
    prompt: async () => {},
    summarize: __SUMMARIZE__,
    command: async (req) => { calls.command.push(req); return { data: {} }; },
  },
  command: { list: async () => ({ data: [{ name: "ship-it" }] }) },
  tui: { executeCommand: async (req) => { calls.tui.push(req); return { data: true }; } },
  config: { providers: async () => ({ data: { providers: [], default: null } }) },
};
const hooks = await KarenAI({ directory: "__DIR__", client });
const idle = () => hooks.event({ event: { type: "session.idle", properties: { sessionID: "s1" } } });
const toolCall = () => hooks["tool.execute.before"]({ tool: "bash", sessionID: "s1", callID: "c1" }, { args: { command: "echo hi" } });
"#;

const RECORD: &str = "async (req) => { calls.summarize.push(req); return { data: true }; }";
const MODELLED: &str = r#"{ model: { providerID: "mock", modelID: "big" }, parentID: null }"#;

/// A user layer holding `gates`, allowing exactly `allow`, and the bridge
/// installed in a project of its own.
fn setup(allow: &[&str], gates: &[(&str, String)]) -> (TestEnv, std::path::PathBuf, String) {
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let list = allow
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    user_layer(
        &env,
        &format!("[actions]\nruntime_commands = [{list}]"),
        gates,
    );
    let (project_dir, plugin_url) = install_bridge(&env);
    (env, project_dir, plugin_url)
}

/// Run `body` after the prelude, with the mock `summarize` and session record
/// given, and parse the JSON line the driver prints. The bridge and the binary
/// it spawns keep their files under the test home.
fn run(
    env: &TestEnv,
    plugin_url: &str,
    project_dir: &std::path::Path,
    session: &str,
    summarize: &str,
    body: &str,
    extra_env: &[(&str, &str)],
) -> Value {
    let driver = format!("{PRELUDE}{body}")
        .replace("__SESSION__", session)
        .replace("__SUMMARIZE__", summarize)
        .replace("__PLUGIN__", plugin_url)
        .replace(
            "__DIR__",
            &project_dir.display().to_string().replace('\\', "/"),
        );
    let script = env.temp_dir.path().join("drive-commands.mjs");
    std::fs::write(&script, driver).unwrap();
    let output = karen::proc::command("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_HOME", env.home_dir.join(".karen"))
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &env.karen_binary)
        .envs(extra_env.iter().copied())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap()
}

/// The `(status, route)` of every ledger line, oldest first. A line her binary
/// wrote has no route; the bridge's lines name the call it made.
fn trail(env: &TestEnv) -> Vec<(String, Option<String>)> {
    runtime_lines(env)
        .iter()
        .map(|line| {
            (
                line["status"].as_str().unwrap_or("").to_string(),
                line["route"].as_str().map(str::to_string),
            )
        })
        .collect()
}

fn entry(status: &str, route: Option<&str>) -> (String, Option<String>) {
    (status.to_string(), route.map(str::to_string))
}

fn skip_without_node() -> bool {
    if runtime_available("node") {
        return false;
    }
    eprintln!("skipping: node not installed");
    true
}

#[test]
#[serial]
fn karen_opencode_bridge_compacts_an_idle_session_through_the_servers_summarize() {
    // `/compact` is the TUI's own `session.summarize` call, so it needs no TUI:
    // an idle review that hands the command back sends it on the model the
    // session runs on. The ledger shows her hand-off and then the bridge's
    // `sent`, and never the arguments' rendering.
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/compact"],
        &[(
            "compact.json",
            command_gate(
                "compact-at-stop",
                "Stop",
                "/compact",
                Some("SECRETARG-{facts.session_id}"),
                true,
            ),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        RECORD,
        r"
await idle();
await nap(600);
console.log(JSON.stringify({ calls }));
",
        &[],
    );
    assert_eq!(
        result["calls"]["summarize"],
        serde_json::json!([{ "path": { "id": "s1" }, "body": { "providerID": "mock", "modelID": "big" } }]),
        "{result}"
    );
    assert_eq!(
        trail(&env),
        [
            entry("handed-off", None),
            entry("sent", Some("session.summarize"))
        ]
    );
    assert!(
        !runtime_log(&env).contains("SECRETARG-s1"),
        "the ledger keeps no rendering: {}",
        runtime_log(&env)
    );
}

#[test]
#[serial]
fn karen_opencode_bridge_runs_a_custom_command_with_its_rendered_arguments() {
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/ship-it"],
        &[(
            "ship.json",
            command_gate(
                "ship-at-stop",
                "Stop",
                "/ship-it",
                Some("now {facts.session_id}"),
                true,
            ),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        RECORD,
        r"
await idle();
await nap(600);
console.log(JSON.stringify({ calls }));
",
        &[],
    );
    assert_eq!(
        result["calls"]["command"],
        serde_json::json!([{ "path": { "id": "s1" }, "body": { "command": "ship-it", "arguments": "now s1" } }]),
        "{result}"
    );
    assert_eq!(result["calls"]["summarize"], serde_json::json!([]));
    assert_eq!(
        trail(&env),
        [
            entry("handed-off", None),
            entry("sent", Some("session.command"))
        ]
    );
}

#[test]
#[serial]
fn karen_opencode_bridge_waits_for_the_turn_to_end_before_it_types() {
    // A command handed back in the middle of a turn waits for the session's
    // next idle: a compaction started under a running tool is a hazard, not a
    // courtesy.
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/compact"],
        &[(
            "compact.json",
            command_gate("compact-at-tool", "PreToolUse", "/compact", None, true),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        RECORD,
        r"
await toolCall();
await nap(400);
const midTurn = calls.summarize.length;
await idle();
await nap(600);
console.log(JSON.stringify({ midTurn, afterIdle: calls.summarize.length }));
",
        &[],
    );
    assert_eq!(result["midTurn"], 0, "nothing is typed mid-turn: {result}");
    assert_eq!(result["afterIdle"], 1, "the idle releases it: {result}");
}

#[test]
#[serial]
fn karen_opencode_bridge_files_a_command_that_never_got_its_idle() {
    // A session that stays busy past the wait never gets the command, and the
    // ledger says so. The wait is two minutes; the test shortens it.
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/compact"],
        &[(
            "compact.json",
            command_gate("compact-at-tool", "PreToolUse", "/compact", None, true),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        RECORD,
        r"
await toolCall();
await nap(1200);
console.log(JSON.stringify({ calls }));
",
        &[("KAREN_COMMAND_WAIT_MS", "300")],
    );
    assert_eq!(result["calls"]["summarize"], serde_json::json!([]));
    assert_eq!(
        trail(&env),
        [
            entry("handed-off", None),
            entry("undelivered:timeout", Some("none"))
        ]
    );
}

#[test]
#[serial]
fn karen_opencode_bridge_types_one_command_per_idle() {
    // A command starts a turn of its own, so the next one waits for that
    // turn's idle instead of piling into it.
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/compact", "/ship-it"],
        &[
            (
                "a.json",
                command_gate("a-compact", "Stop", "/compact", None, true),
            ),
            (
                "b.json",
                command_gate("b-ship", "Stop", "/ship-it", Some("go"), true),
            ),
        ],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        RECORD,
        r"
await idle();
await nap(600);
const first = { summarize: calls.summarize.length, command: calls.command.length };
await idle();
await nap(600);
const second = { summarize: calls.summarize.length, command: calls.command.length };
console.log(JSON.stringify({ first, second }));
",
        &[],
    );
    assert_eq!(
        result["first"],
        serde_json::json!({ "summarize": 1, "command": 0 }),
        "{result}"
    );
    assert_eq!(
        result["second"],
        serde_json::json!({ "summarize": 1, "command": 1 }),
        "the second waited for the next idle: {result}"
    );
}

#[test]
#[serial]
fn karen_opencode_bridge_sends_a_tui_builtin_but_never_calls_it_delivered() {
    // The server answers `true` to `tui.executeCommand` whether or not a TUI is
    // listening, so the ledger records the call as sent and unconfirmed.
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/new"],
        &[(
            "new.json",
            command_gate("new-at-stop", "Stop", "/new", None, true),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        RECORD,
        r"
await idle();
await nap(600);
console.log(JSON.stringify({ calls }));
",
        &[],
    );
    assert_eq!(
        result["calls"]["tui"],
        serde_json::json!([{ "body": { "command": "session.new" } }]),
        "{result}"
    );
    assert_eq!(
        trail(&env),
        [
            entry("handed-off", None),
            entry("sent-unconfirmed", Some("tui.executeCommand"))
        ]
    );
}

#[test]
#[serial]
fn karen_opencode_bridge_names_a_command_it_does_not_know() {
    // Not `/compact`, not one of the server's commands, not a TUI built-in
    // she has a name for: nothing is guessed, and the ledger says so.
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/frobnicate"],
        &[(
            "frob.json",
            command_gate("frob-at-stop", "Stop", "/frobnicate", None, true),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        RECORD,
        r"
await idle();
await nap(600);
console.log(JSON.stringify({ calls }));
",
        &[],
    );
    assert_eq!(
        result["calls"],
        serde_json::json!({ "summarize": [], "command": [], "tui": [] }),
        "{result}"
    );
    assert_eq!(
        trail(&env),
        [
            entry("handed-off", None),
            entry("undelivered:unknown-command", Some("none"))
        ]
    );
}

#[test]
#[serial]
fn karen_opencode_bridge_reads_the_error_a_failed_request_answers_with() {
    // The SDK returns an error instead of throwing it, so a request the server
    // refused is read from the answer, not from a catch that never fires.
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/compact"],
        &[(
            "compact.json",
            command_gate("compact-at-stop", "Stop", "/compact", None, true),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        "async () => ({ error: { name: 'BadRequest' }, response: { status: 400 } })",
        r"
await idle();
await nap(800);
console.log(JSON.stringify({}));
",
        &[],
    );
    let _ = result;
    assert_eq!(
        trail(&env),
        [
            entry("handed-off", None),
            entry("sent", Some("session.summarize")),
            entry("undelivered:error", Some("session.summarize")),
        ]
    );
}

#[test]
#[serial]
fn karen_opencode_bridge_will_not_compact_on_a_model_it_cannot_name() {
    // `session.summarize` wants a provider and a model. A session that names
    // neither is not compacted on a guess.
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/compact"],
        &[(
            "compact.json",
            command_gate("compact-at-stop", "Stop", "/compact", None, true),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        "{ parentID: null }",
        RECORD,
        r"
await idle();
await nap(600);
console.log(JSON.stringify({ calls }));
",
        &[],
    );
    assert_eq!(result["calls"]["summarize"], serde_json::json!([]));
    assert_eq!(
        trail(&env),
        [
            entry("handed-off", None),
            entry("undelivered:no-model", Some("session.summarize"))
        ]
    );
}

#[test]
#[serial]
fn karen_opencode_bridge_files_the_commands_a_deleted_session_never_ran() {
    if skip_without_node() {
        return;
    }
    let (env, dir, url) = setup(
        &["/compact"],
        &[(
            "compact.json",
            command_gate("compact-at-tool", "PreToolUse", "/compact", None, true),
        )],
    );
    let result = run(
        &env,
        &url,
        &dir,
        MODELLED,
        RECORD,
        r#"
await toolCall();
await hooks.event({ event: { type: "session.deleted", properties: { sessionID: "s1" } } });
await nap(300);
console.log(JSON.stringify({ calls }));
"#,
        &[],
    );
    assert_eq!(result["calls"]["summarize"], serde_json::json!([]));
    assert_eq!(
        trail(&env),
        [
            entry("handed-off", None),
            entry("undelivered:session-deleted", Some("none"))
        ]
    );
}
