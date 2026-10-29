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
