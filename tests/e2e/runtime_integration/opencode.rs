// tests/e2e/runtime_integration/opencode.rs
// The Karen who tests real OpenCode integration because closed doors don't
// stop her from finding the mess.

use crate::e2e::common::{
    TestEnv, assert_exit_code, assert_json_ok, run_command, runtime_available,
};
use serial_test::serial;
use std::fs;
use std::process::Command;

#[test]
#[serial]
fn karen_runtime_opencode_plugin_bridges_gates_and_rules() {
    // The Karen who checks the plugin the way OpenCode will run it: loaded as
    // a module, called through its hooks, talking to the real binary. A
    // plugin that only exists on disk is a decoration, not a guardrail.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );

    let script = env.temp_dir.path().join("drive-plugin.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const hooks = await KarenAI({{ directory: {dir:?} }});
const result = {{}};
async function before(command) {{
  try {{
    await hooks["tool.execute.before"]({{ tool: "bash", sessionID: "s1", callID: command }}, {{ args: {{ command }} }});
    return "allowed";
  }} catch (e) {{ return "denied: " + e.message; }}
}}
result.attributed = await before("git commit -m 'x' -m 'Co-Authored-By: Claude <noreply@anthropic.com>'");
result.clean = await before("git status");
const system = [];
await hooks["experimental.chat.system.transform"]({{ sessionID: "s1" }}, {{ system }});
await hooks["experimental.chat.system.transform"]({{ sessionID: "s1" }}, {{ system }});
result.rules = system;
const after = {{ title: "", output: "ok", metadata: {{}} }};
await hooks["tool.execute.after"]({{ tool: "bash", sessionID: "s1", callID: "c9", args: {{ command: "git status" }} }}, after);
result.after = after.output;
console.log(JSON.stringify(result));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();

    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &env.karen_binary)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    let attributed = result["attributed"].as_str().unwrap();
    assert!(
        attributed.starts_with("denied: Karen-AI:") && attributed.contains("attribution"),
        "the commit-attribution gate must block through the plugin: {attributed}"
    );
    assert_eq!(result["clean"], "allowed");
    let rules = result["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 2, "rules must ride along on every request");
    assert!(
        rules[0]
            .as_str()
            .unwrap()
            .contains("Edit and Commit Core Rules")
    );
    assert_eq!(rules[0], rules[1]);
    assert_eq!(result["after"], "ok", "a clean call gains no notices");
}

#[test]
#[serial]
fn karen_runtime_opencode_plugin_reports_missing_policies_once_per_session() {
    // The Karen who will not stand guard in silence: with no policy root the
    // plugin still lets everything through, but each session hears once that
    // she is off duty, through a tool result or, for a session that only
    // idles, a prompt. Once, because a nag nobody reads is just more silence.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );

    let script = env.temp_dir.path().join("drive-missing.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const prompts = [];
const history = [];
const client = {{ session: {{
  messages: async ({{ path }}) => ({{ data: history }}),
  get: async ({{ path }}) => ({{ data: {{ id: path.id }} }}),
  prompt: async ({{ path, body }}) => {{ prompts.push(path.id + ": " + body.parts.map((p) => p.text).join("")); return {{ data: {{}} }}; }},
}} }};
const hooks = await KarenAI({{ directory: {dir:?}, client }});
const afters = [];
for (const callID of ["c1", "c2"]) {{
  await hooks["tool.execute.before"]({{ tool: "bash", sessionID: "s1", callID }}, {{ args: {{ command: "git status" }} }});
  const after = {{ title: "", output: "ok", metadata: {{}} }};
  await hooks["tool.execute.after"]({{ tool: "bash", sessionID: "s1", callID, args: {{ command: "git status" }} }}, after);
  afters.push(after.output);
}}
const say = (id, text) => history.push({{ info: {{ id, role: "assistant" }}, parts: [{{ type: "text", text }}] }});
const idle = (sid) => hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: sid }} }} }});
say("m1", "Standing by."); await idle("s1"); await idle("s2");
say("m2", "Still standing by."); await idle("s2");
console.log(JSON.stringify({{ afters, prompts }}));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();

    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_POLICIES", env.policies_dir.join("not-here"))
        .env("KAREN_BIN", &env.karen_binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    let afters = result["afters"].as_array().unwrap();
    assert!(
        afters[0]
            .as_str()
            .unwrap()
            .contains("the project layer is missing"),
        "{result}"
    );
    assert_eq!(afters[1], "ok", "told once per session: {result}");
    let prompts = result["prompts"].as_array().unwrap();
    assert_eq!(
        prompts.len(),
        1,
        "s1 already knows; s2 hears once: {result}"
    );
    assert!(
        prompts[0]
            .as_str()
            .unwrap()
            .starts_with("s2: [Karen-AI] Karen-AI policy merge: the project layer is missing"),
        "{result}"
    );
}

#[test]
#[serial]
fn karen_runtime_opencode_plugin_never_wakes_a_session_for_a_gate_warning() {
    // An allowed stop that carries a gate's warning once woke the session
    // with that warning, so every reply ended in another idle, another
    // review and another wake. Only a hold, and Karen's own status lines
    // once each, wake a session; a warning is not a reason to keep talking.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    fs::write(
        env.policies_dir.join("gates").join("oc-warn.json"),
        r#"{ "id": "oc-closing-note", "event": "Stop", "priority": 50, "match": { "all": [ { "type": "always" } ] }, "action": { "type": "deny_once_then_warn", "first_reason": "Closing check before you stop.", "second_reason": "Closing check noted." }, "state": { "track": "session", "key": "oc-closing-note", "max_strikes": 2 } }"#,
    )
    .unwrap();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );

    let script = env.temp_dir.path().join("drive-warn.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const prompts = [];
const history = [];
const client = {{ session: {{
  messages: async ({{ path }}) => ({{ data: history }}),
  get: async ({{ path }}) => ({{ data: {{ id: path.id }} }}),
  prompt: async ({{ path, body }}) => {{ prompts.push(path.id + ": " + body.parts.map((p) => p.text).join("")); return {{ data: {{}} }}; }},
}} }};
const hooks = await KarenAI({{ directory: {dir:?}, client }});
const say = (id, text) => history.push({{ info: {{ id, role: "assistant" }}, parts: [{{ type: "text", text }}] }});
const idle = () => hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: "s1" }} }} }});
const counts = [];
for (const id of ["m1", "m2", "m3", "m4"]) {{ say(id, "Done with " + id + "."); await idle(); counts.push(prompts.length); }}
console.log(JSON.stringify({{ counts, prompts }}));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();

    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &env.karen_binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    // The first stop is held once; every later stop only warns, and stays stopped.
    assert_eq!(
        result["counts"],
        serde_json::json!([1, 1, 1, 1]),
        "{result}"
    );
    assert!(
        result["prompts"][0]
            .as_str()
            .unwrap()
            .contains("Closing check before you stop."),
        "{result}"
    );
}

#[test]
#[serial]
fn karen_runtime_opencode_plugin_keeps_payloads_in_her_own_run_directory() {
    // A payload holds tool input and the session's own words. It is written
    // under `<Karen home>/run`, readable only by its owner, under a name
    // nobody can predict, and removed once she has answered.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    let script = env.temp_dir.path().join("drive-run.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const hooks = await KarenAI({{ directory: {dir:?} }});
let verdict = "allowed";
try {{ await hooks["tool.execute.before"]({{ tool: "bash", sessionID: "s1", callID: "c1" }}, {{ args: {{ command: "rm -rf build" }} }}); }} catch (e) {{ verdict = e.message; }}
console.log(JSON.stringify({{ verdict }}));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();
    fs::write(
        env.policies_dir.join("gates").join("oc-rm.json"),
        r#"{ "id": "oc-rm", "event": "PreToolUse", "tool": "Bash", "match": { "all": [ { "type": "command_regex", "pattern": "^rm " } ] }, "action": { "type": "deny", "reason": "no rm" } }"#,
    )
    .unwrap();
    let karen_home = env.temp_dir.path().join("karen-home");
    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_HOME", &karen_home)
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &env.karen_binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    assert!(
        result["verdict"].as_str().unwrap().contains("no rm"),
        "the payload reached her: {result}"
    );
    let run = karen_home.join("run");
    assert!(run.is_dir(), "payloads are written under {}", run.display());
    assert_eq!(
        fs::read_dir(&run).unwrap().count(),
        0,
        "every payload is removed once she has answered"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&run).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "only the owner reads the run directory");
    }
}

#[test]
#[serial]
fn karen_runtime_opencode_plugin_holds_idle_sessions_with_stop_gates() {
    // The Karen who stops you at the door: OpenCode has no Stop hook, so the
    // plugin turns `session.idle` into a Stop review and, when a gate blocks,
    // sends the reason back into the session. A Stop gate that never fires on
    // OpenCode was a promise with nobody home.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    fs::write(
        env.policies_dir.join("gates").join("oc-stop.json"),
        r#"{ "gates": [
  { "id": "oc-closing-check", "event": "Stop", "priority": 50, "match": { "all": [ { "type": "always" } ], "none": [ { "type": "field_equals", "field": "stop_hook_active", "value": true } ] }, "action": { "type": "deny", "reason": "Closing check before you stop." } },
  { "id": "oc-open-question", "event": "Stop", "priority": 45, "match": { "all": [ { "type": "always" } ], "none": [ { "type": "field_regex", "field": "last_assistant_message", "pattern": "\\[Q-7\\]" } ] }, "action": { "type": "deny", "reason": "Re-ask open question Q-7." } }
] }"#,
    )
    .unwrap();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );

    let script = env.temp_dir.path().join("drive-idle.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const prompts = [];
const history = [];
const client = {{ session: {{
  messages: async ({{ path }}) => ({{ data: history }}),
  get: async ({{ path }}) => ({{ data: {{ id: path.id, parentID: path.id === "child" ? "s1" : undefined }} }}),
  prompt: async ({{ path, body }}) => {{ prompts.push(path.id + ": " + body.parts.map((p) => p.text).join("")); return {{ data: {{}} }}; }},
}} }};
const hooks = await KarenAI({{ directory: {dir:?}, client }});
const say = (id, text) => history.push({{ info: {{ id, role: "assistant" }}, parts: [{{ type: "text", text }}] }});
const idle = () => hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: "s1" }} }} }});
const status = () => hooks.event({{ event: {{ type: "session.status", properties: {{ sessionID: "s1", status: {{ type: "idle" }} }} }} }});
const counts = [];
say("m1", "All done."); await idle(); await status(); counts.push(prompts.length);
say("m2", "Listed the open work."); await idle(); counts.push(prompts.length);
say("m3", "[Q-7] Approve the plan?"); await idle(); counts.push(prompts.length);
say("m4", "Next task done."); await idle(); counts.push(prompts.length);
await hooks.event({{ event: {{ type: "session.updated", properties: {{ sessionID: "s1" }} }} }}); counts.push(prompts.length);
say("m5", "Subagent result."); await hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: "child" }} }} }}); counts.push(prompts.length);
console.log(JSON.stringify({{ counts, prompts }}));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();

    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &env.karen_binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    // m1 blocked (one prompt despite the duplicate status event); m2 is a
    // continuation, so the closing check lets it pass but Q-7 still holds it;
    // m3 re-asks Q-7 and may stop; m4 is a fresh stop and is reviewed again;
    // other events are ignored; a child session is a SubagentStop, which these
    // Stop gates do not review.
    assert_eq!(
        result["counts"],
        serde_json::json!([1, 2, 2, 3, 3, 3]),
        "{result}"
    );
    let prompts = result["prompts"].as_array().unwrap();
    assert!(
        prompts
            .iter()
            .all(|p| p.as_str().unwrap().starts_with("s1: [Karen-AI] ")),
        "{result}"
    );
    assert!(
        prompts[1].as_str().unwrap().contains("Q-7"),
        "the continuation is held by the open question, not the closing check: {result}"
    );
}

#[test]
#[serial]
fn karen_runtime_opencode_wake_runs_on_the_sessions_current_model() {
    // The Karen who wakes you on the phone you're holding now, not the one you
    // had at check-in: a prompt without a model runs on the session's creation-
    // time model, so the hold names the live session model, then the
    // OPENCODE_CONFIG model, and sends none only when neither is known. The
    // bridge spawns asynchronously: spawnSync of the same binary timed out on
    // every call under OpenCode's embedded runtime.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    fs::write(
        env.policies_dir.join("gates").join("oc-hold.json"),
        r#"{ "gates": [ { "id": "oc-hold-all", "event": "Stop", "priority": 50, "match": { "all": [ { "type": "always" } ] }, "action": { "type": "deny", "reason": "Keep going." } } ] }"#,
    )
    .unwrap();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let source = fs::read_to_string(&plugin).unwrap();
    assert!(
        !source.contains("spawnSync(") && !source.contains("{ spawnSync"),
        "the bridge must spawn asynchronously"
    );
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    let config = env.temp_dir.path().join("opencode-lane.json");
    fs::write(&config, r#"{ "model": "cfgprov/cfg-model" }"#).unwrap();

    let script = env.temp_dir.path().join("drive-wake.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const sent = [];
const history = {{ live: [], cfg: [] }};
const client = {{ session: {{
  messages: async ({{ path }}) => ({{ data: history[path.id] }}),
  get: async ({{ path }}) => ({{ data: path.id === "live" ? {{ id: path.id, model: {{ providerID: "liveprov", modelID: "live-model" }} }} : {{ id: path.id }} }}),
  prompt: async ({{ path, body }}) => {{ sent.push({{ id: path.id, model: body.model ?? null }}); return {{ data: {{}} }}; }},
}} }};
const hooks = await KarenAI({{ directory: {dir:?}, client }});
for (const id of ["live", "cfg"]) {{
  history[id].push({{ info: {{ id: id + "-m1", role: "assistant" }}, parts: [{{ type: "text", text: "Stopping." }}] }});
  await hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: id }} }} }});
}}
console.log(JSON.stringify(sent));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();

    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &env.karen_binary)
        .env("OPENCODE_CONFIG", &config)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sent: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    assert_eq!(
        sent,
        serde_json::json!([
            { "id": "live", "model": { "providerID": "liveprov", "modelID": "live-model" } },
            { "id": "cfg", "model": { "providerID": "cfgprov", "modelID": "cfg-model" } }
        ]),
        "{sent}"
    );
}

#[test]
#[serial]
fn karen_runtime_opencode_stop_payload_counts_consecutive_holds() {
    // The Karen who asks more than once but knows when to stop asking: a
    // gate exempting every continuation can be outwaited with one more
    // announcement, so the payload counts the holds in a row and a gate can
    // insist twice before it lets go. The count resets when a stop passes.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    fs::write(
        env.policies_dir.join("gates").join("oc-holds.json"),
        r#"{ "gates": [
  { "id": "oc-status", "event": "Stop", "priority": 50, "match": { "all": [ { "type": "always" } ], "none": [ { "type": "field_regex", "field": "last_assistant_message", "pattern": "(?m)^(DONE|WAITING|BLOCKED):" }, { "type": "field_equals", "field": "stop_hold_count", "value": 2 } ] }, "action": { "type": "deny", "reason": "status line" } }
] }"#,
    )
    .unwrap();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );

    let script = env.temp_dir.path().join("drive-holds.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const prompts = [];
const history = [];
const client = {{ session: {{
  messages: async ({{ path }}) => ({{ data: history }}),
  get: async ({{ path }}) => ({{ data: {{ id: path.id }} }}),
  prompt: async ({{ path, body }}) => {{ prompts.push(body.parts.map((p) => p.text).join("")); return {{ data: {{}} }}; }},
}} }};
const hooks = await KarenAI({{ directory: {dir:?}, client }});
const say = (id, text) => history.push({{ info: {{ id, role: "assistant" }}, parts: [{{ type: "text", text }}] }});
const idle = () => hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: "s1" }} }} }});
const counts = [];
for (const [id, text] of [["m1", "Standing by."], ["m2", "Arming the monitor."], ["m3", "Closing out."], ["m4", "Waiting quietly."], ["m5", "WAITING: on lora."]]) {{
  say(id, text); await idle(); counts.push(prompts.length);
}}
console.log(JSON.stringify({{ counts }}));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();

    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &env.karen_binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    // m1 and m2 are held (counts 0 and 1); m3 arrives with two holds in a row
    // and passes, resetting the count; m4 is held again; m5 has a status line.
    assert_eq!(
        result["counts"],
        serde_json::json!([1, 2, 2, 3, 3]),
        "{result}"
    );
}

#[test]
#[serial]
fn karen_runtime_opencode_stop_payload_carries_todo_state() {
    // The Karen who reads your checklist, not just your closing remarks: an
    // agent can word its way past a text gate, but its own todo list is a
    // record it has to keep straight.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    fs::write(
        env.policies_dir.join("gates").join("oc-todos.json"),
        r#"{ "gates": [
  { "id": "oc-open", "event": "Stop", "priority": 50, "match": { "all": [ { "type": "field_regex", "field": "open_todos", "pattern": "\\S" } ] }, "action": { "type": "deny", "reason": "open" } },
  { "id": "oc-stale", "event": "Stop", "priority": 45, "match": { "all": [ { "type": "field_equals", "field": "todos_updated", "value": false }, { "type": "field_regex", "field": "last_assistant_message", "pattern": "DONE:" } ] }, "action": { "type": "deny", "reason": "stale" } },
  { "id": "oc-noplan", "event": "Stop", "priority": 40, "match": { "all": [ { "type": "field_equals", "field": "todo_count", "value": 0 }, { "type": "field_regex", "field": "last_assistant_message", "pattern": "DONE:" } ] }, "action": { "type": "deny", "reason": "no plan" } }
] }"#,
    )
    .unwrap();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );

    let script = env.temp_dir.path().join("drive-todos.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const prompts = [];
const history = {{ s1: [], s3: [] }};
let todos = [];
const client = {{ session: {{
  messages: async ({{ path }}) => ({{ data: history[path.id] }}),
  get: async ({{ path }}) => ({{ data: {{ id: path.id }} }}),
  todo: async ({{ path }}) => {{ if (path.id === "s3") throw new Error("no todo api"); return {{ data: todos }}; }},
  prompt: async ({{ path, body }}) => {{ prompts.push(path.id + ": " + body.parts.map((p) => p.text).join("")); return {{ data: {{}} }}; }},
}} }};
const hooks = await KarenAI({{ directory: {dir:?}, client }});
const say = (sid, id, text) => history[sid].push({{ info: {{ id, role: "assistant" }}, parts: [{{ type: "text", text }}] }});
const idle = (sid) => hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: sid }} }} }});
const counts = [];
say("s1", "m1", "DONE: a"); await idle("s1"); counts.push(prompts.length);
todos = [{{ content: "x", status: "in_progress", priority: "high" }}];
say("s1", "m2", "working"); await idle("s1"); counts.push(prompts.length);
todos = [{{ content: "x", status: "completed", priority: "high" }}];
say("s1", "m3", "DONE: x"); await idle("s1"); counts.push(prompts.length);
say("s1", "m4", "DONE: y"); await idle("s1"); counts.push(prompts.length);
say("s3", "n1", "DONE: z"); await idle("s3"); counts.push(prompts.length);
console.log(JSON.stringify({{ counts, prompts }}));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();

    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &env.karen_binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    // m1: DONE with no plan; m2: an open item; m3: the plan changed and is
    // closed; m4: DONE again on an unchanged plan; s3: no todo API, so the
    // fields are absent and no todo gate can fire.
    assert_eq!(
        result["counts"],
        serde_json::json!([1, 2, 2, 3, 3]),
        "{result}"
    );
    assert_eq!(
        result["prompts"],
        serde_json::json!([
            "s1: [Karen-AI] no plan",
            "s1: [Karen-AI] open",
            "s1: [Karen-AI] stale"
        ]),
        "{result}"
    );
}

#[test]
#[serial]
fn karen_runtime_opencode_plugin_logs_every_open_failure() {
    // The Karen who keeps a diary of the times she could not stand guard:
    // every failure still lets the session through, but each one leaves a
    // line in her log, so an unheld stop is explained from evidence.
    if !runtime_available("node") {
        eprintln!("skipping: node not installed");
        return;
    }
    let env = TestEnv::new();
    fs::write(
        env.policies_dir.join("gates").join("oc-always.json"),
        r#"{ "gates": [
  { "id": "oc-always", "event": "Stop", "priority": 50, "match": { "all": [ { "type": "always" } ] }, "action": { "type": "deny", "reason": "status line" } }
] }"#,
    )
    .unwrap();
    let project_dir = env.temp_dir.path().join("opencode-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "opencode", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let plugin = project_dir.join(data["installed"][0]["path"].as_str().unwrap());
    let plugin_url = format!(
        "file:///{}",
        plugin
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );

    let script = env.temp_dir.path().join("drive-failures.mjs");
    fs::write(
        &script,
        format!(
            r#"const {{ KarenAI }} = await import({plugin_url:?});
const history = [{{ info: {{ id: "m1", role: "assistant" }}, parts: [{{ type: "text", text: "Standing by." }}] }}];
const client = {{ session: {{
  messages: async ({{ path }}) => {{ if (path.id === "broken") throw new Error("store offline"); return {{ data: history }}; }},
  get: async ({{ path }}) => ({{ data: {{ id: path.id }} }}),
  prompt: async () => {{ throw new Error("session busy"); }},
}} }};
const hooks = await KarenAI({{ directory: {dir:?}, client }});
await hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: "s1" }} }} }});
await hooks.event({{ event: {{ type: "session.idle", properties: {{ sessionID: "broken" }} }} }});
await hooks["tool.execute.before"]({{ tool: "bash", sessionID: "s1", callID: "c1" }}, {{ args: {{ command: "git status" }} }});
await new Promise((r) => setTimeout(r, 100));
"#,
            dir = project_dir.display().to_string(),
        ),
    )
    .unwrap();

    let run = |bin: &std::path::Path, log: &std::path::Path| {
        let output = Command::new("node")
            .arg(&script)
            .env("HOME", &env.home_dir)
            .env("KAREN_POLICIES", &env.policies_dir)
            .env("KAREN_BIN", bin)
            .env("KAREN_LOG", log)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "node driver failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(log).unwrap_or_default()
    };

    let held = run(&env.karen_binary, &env.temp_dir.path().join("held.log"));
    assert!(
        held.contains("session=s1 session.prompt (hold not delivered): session busy"),
        "{held}"
    );
    assert!(
        held.contains("session=broken session.messages on idle: store offline"),
        "{held}"
    );

    let missing = env.temp_dir.path().join("no-karen.exe");
    let offline = run(&missing, &env.temp_dir.path().join("offline.log"));
    assert!(
        offline.contains("session=s1 spawn") && offline.contains("on PreToolUse"),
        "{offline}"
    );

    // Without KAREN_LOG the log lives in her home: <KAREN_HOME>/logs.
    let karen_home = env.temp_dir.path().join("karen-home");
    let output = Command::new("node")
        .arg(&script)
        .env("HOME", &env.home_dir)
        .env("KAREN_HOME", &karen_home)
        .env("KAREN_POLICIES", &env.policies_dir)
        .env("KAREN_BIN", &missing)
        .env_remove("KAREN_LOG")
        .output()
        .unwrap();
    assert!(output.status.success(), "node driver failed");
    let default_log =
        fs::read_to_string(karen_home.join("logs").join("karen-opencode.log")).unwrap_or_default();
    assert!(default_log.contains("on PreToolUse"), "{default_log}");
}
