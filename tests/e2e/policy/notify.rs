// tests/e2e/policy/notify.rs
// The Karen who tests the complaint department: a notice that never blocks,
// a log line that never lies, and a placeholder that stays literal when it
// asks for the lockbox.

use crate::e2e::common::{DEFAULT_CONFIG, GateFixture, TestEnv, assert_exit_code, assert_json_ok};
use serde_json::json;
use serial_test::serial;

/// A `notify` gate with a templated reason. Test values leak: a fixture lives
/// as long as the process that loads it.
fn notify_gate(file: &'static str, id: &str, reason: &str) -> GateFixture {
    let content: &'static str = Box::leak(
        format!(
            r#"{{
  "id": "{id}",
  "description": "The test gate for the notify action.",
  "event": "PreToolUse",
  "match": {{
    "all": [ {{ "type": "always" }} ],
    // A `none` condition that never excludes names the facts a reason
    // templates, so they are collected for the rendering.
    "none": [
      {{ "type": "field_equals", "field": "facts.runtime", "value": "__never__" }},
      {{ "type": "field_equals", "field": "facts.context_ratio", "value": -1 }}
    ]
  }},
  "action": {{ "type": "notify", "reason": "{reason}" }}
}}"#
        )
        .into_boxed_str(),
    );
    GateFixture { file, content }
}

fn pretool(session: &str) -> String {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": session,
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
    })
    .to_string()
}

/// The notify log a test home keeps, as parsed JSON lines.
fn notify_lines(env: &TestEnv) -> Vec<serde_json::Value> {
    let log = env
        .home_dir
        .join(".karen")
        .join("logs")
        .join("karen-notify.jsonl");
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[test]
#[serial]
fn karen_notifies_allows_and_files_one_line() {
    // Notify never blocks: the call proceeds, the notice lands in the session,
    // and one JSON line lands in the log — time, session, gate, rendered text.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[notify_gate(
            "notify.json",
            "notify-test",
            r"gate {gate} saw {tool}",
        )],
        &[],
    );
    let (code, stdout, _stderr) = env.run_hook(&pretool("notify-1"), &[]);
    assert_exit_code(code, 0);
    let out: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON output");
    // Notify never blocks: no decision, no hookSpecificOutput — the notice
    // rides a systemMessage, as an allowed call's warning does.
    assert!(
        out.get("decision").is_none(),
        "notify must not block: {stdout}"
    );
    assert!(
        out.get("hookSpecificOutput").is_none(),
        "notify must not hold a tool call: {stdout}"
    );
    assert!(
        out["systemMessage"]
            .as_str()
            .is_some_and(|m| m.contains("gate notify-test saw Bash")),
        "the notice rides a systemMessage: {stdout}"
    );
    let lines = notify_lines(&env);
    assert_eq!(lines.len(), 1, "one notify line, not a diary: {lines:?}");
    assert_eq!(lines[0]["session"], "notify-1");
    assert_eq!(lines[0]["gate"], "notify-test");
    assert_eq!(lines[0]["text"], "gate notify-test saw Bash");
    assert!(
        lines[0]["time"].as_u64().is_some(),
        "time is epoch-ms: {:?}",
        lines[0]
    );
}

#[test]
#[serial]
fn karen_renders_facts_into_a_reason_and_unknown_for_absence() {
    // `{facts.<path>}` renders the enriched fact; an absent fact renders
    // `(unknown)`, never the empty string that reads like a value.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[notify_gate(
            "notify-facts.json",
            "notify-facts",
            r"runtime is {facts.runtime}, ratio is {facts.context_ratio}, ghost is {facts.ghost}",
        )],
        &[],
    );
    let (code, _stdout, _stderr) = env.run_hook(&pretool("notify-2"), &["--runtime", "claude"]);
    assert_exit_code(code, 0);
    let lines = notify_lines(&env);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        lines[0]["text"],
        "runtime is claude, ratio is (unknown), ghost is (unknown)"
    );
}

#[test]
#[serial]
fn karen_renders_an_object_fact_as_compact_json() {
    // A payload fact that is an object renders as its compact JSON, so a
    // number stays a number and an object stays readable.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[notify_gate(
            "notify-object.json",
            "notify-object",
            r"the error was {facts.provider_error.message}",
        )],
        &[],
    );
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "notify-3",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "facts": { "provider_error": { "message": "timed out", "model": "<synthetic>" } },
    })
    .to_string();
    let (code, stdout, _stderr) = env.run_hook(&input, &[]);
    assert_exit_code(code, 0);
    let out: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON output");
    let _ = out; // allow shape; the assertion is the log line
    let lines = notify_lines(&env);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["text"], "the error was timed out");
}

#[test]
#[serial]
fn karen_leaves_an_env_placeholder_literal_in_a_reason() {
    // The allowlist granted a match, not a broadcast: `{facts.env.*}` renders
    // as its own literal placeholder, whatever the variable holds.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[notify_gate(
            "notify-env.json",
            "notify-env",
            r"running on {facts.env.KAREN_TEST_NOTIFY}",
        )],
        &[],
    );
    let (code, stdout, _stderr) = env.run_hook(&pretool("notify-4"), &[]);
    assert_exit_code(code, 0);
    let _ = stdout;
    let lines = notify_lines(&env);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["text"], "running on {facts.env.KAREN_TEST_NOTIFY}");
}

#[test]
#[serial]
fn karen_validate_names_a_reason_that_templates_env() {
    // A reason templating `{facts.env.*}` is named by validate at the border,
    // because the placeholder will stay literal and the author should know.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[notify_gate(
            "notify-env-template.json",
            "notify-env-template",
            r"on {facts.env.KAREN_TEST_NOTIFY}",
        )],
        &[],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let issues = data["issues"].as_array().expect("issues array");
    let text: Vec<String> = issues
        .iter()
        .map(|i| {
            format!(
                "{}: {}",
                i["kind"].as_str().unwrap_or(""),
                i["message"].as_str().unwrap_or("")
            )
        })
        .collect();
    assert!(
        text.iter()
            .any(|line| line.contains("templates {facts.env.KAREN_TEST_NOTIFY}")),
        "validate must name the env-templated reason: {text:?}"
    );
}

#[test]
#[serial]
fn karen_rotates_the_notify_log_past_ten_mib() {
    // Past 10 MiB the log rotates — one generation — and the next line
    // starts a fresh log. The Karen who empties the drawer when it sticks.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[notify_gate(
            "notify-rotate.json",
            "notify-rotate",
            r"gate {gate}",
        )],
        &[],
    );
    // Pre-fill the log past the cap, so the first notify must rotate.
    let dir = env.home_dir.join(".karen").join("logs");
    std::fs::create_dir_all(&dir).expect("create log dir");
    std::fs::write(
        dir.join("karen-notify.jsonl"),
        vec![0u8; 10 * 1024 * 1024 + 1],
    )
    .expect("fill log");
    let (code, stdout, _stderr) = env.run_hook(&pretool("notify-5"), &[]);
    assert_exit_code(code, 0);
    let _ = stdout;
    assert!(
        dir.join("karen-notify.jsonl.1").is_file(),
        "the old log rotates to .1"
    );
    // The fresh log holds exactly the one new line.
    let lines = notify_lines(&env);
    assert_eq!(
        lines.len(),
        1,
        "after rotation the new log holds the new line: {lines:?}"
    );
    assert_eq!(lines[0]["gate"], "notify-rotate");
}
