use std::io::Write;
use std::process::Command;

fn binary() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_name = if cfg!(windows) { "karen.exe" } else { "karen" };
    let path = std::path::Path::new(manifest_dir)
        .join("target")
        .join("release")
        .join(binary_name);
    path.to_string_lossy().to_string()
}

// The Karen who brings her own evidence to every interview: each test gets
// a fresh policy root with her fixture gates and rules — no borrowing the
// author's filing cabinet, which never shipped anyway.
fn fixture_policies() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp policy root");
    let root = dir.path();
    std::fs::write(root.join("config.toml"), "[engine]\nfail_open = true\n").expect("config");
    let gates = root.join("gates");
    std::fs::create_dir_all(&gates).expect("gates dir");
    std::fs::write(
        gates.join("force-push.json"),
        r#"{ "id": "fixture-force-push", "event": "PreToolUse", "priority": 90, "match": { "all": [ { "type": "command_regex", "pattern": "git\\s+push.*--force" } ] }, "action": { "type": "deny", "reason": "Fixture gate: no force pushes, not even on Fridays." } }"#,
    )
    .expect("gate");
    std::fs::write(
        gates.join("jq-yaml.json"),
        r#"{ "id": "fixture-jq-yaml", "event": "PreToolUse", "priority": 60, "match": { "all": [ { "type": "command_regex", "pattern": "jq\\s+.*\\.ya?ml" } ] }, "action": { "type": "deny_once_then_warn", "first_reason": "Use yq for YAML.", "second_reason": "Allowing jq on retry — she saw the whole thing." }, "state": { "key": "fixture-jq-yaml", "max_strikes": 2 } }"#,
    )
    .expect("gate");
    std::fs::write(
        gates.join("head-rewrite.json"),
        r#"{ "id": "fixture-head-rewrite", "event": "PreToolUse", "priority": 40, "match": { "all": [ { "type": "command_regex", "pattern": "^head\\s+-n?\\s*\\d+" } ] }, "action": { "type": "modify", "reason": "Fixture gate: rewriting head to Select-Object for portability.", "modifier": "head-to-select-object" } }"#,
    )
    .expect("gate");
    std::fs::write(
        gates.join("closing-check.json"),
        r#"{ "id": "fixture-closing-check", "event": "Stop", "priority": 50, "match": { "all": [ { "type": "always" } ], "none": [ { "type": "field_equals", "field": "stop_hook_active", "value": true } ] }, "action": { "type": "deny", "reason": "Fixture gate: closing check before you stop." } }"#,
    )
    .expect("gate");
    std::fs::write(
        gates.join("open-question.json"),
        r#"{ "id": "fixture-open-question", "event": "SubagentStop", "priority": 45, "match": { "all": [ { "type": "always" } ], "none": [ { "type": "field_regex", "field": "last_assistant_message", "pattern": "\\[Q-7\\]" } ] }, "action": { "type": "deny", "reason": "Fixture gate: re-ask open question Q-7." } }"#,
    )
    .expect("gate");
    let rules = root.join("rules");
    std::fs::create_dir_all(&rules).expect("rules dir");
    for n in 1..=4 {
        let body = format!(
            "# Standing rule {n}\n\n{}\n",
            "Do the thing correctly, in writing. ".repeat(250)
        );
        std::fs::write(rules.join(format!("rule-{n}.md")), body).expect("rule");
    }
    dir
}

fn run_hook(input: &str, args: &[&str]) -> (i32, String, String) {
    // The Karen who isolates her state: tests never write to the real
    // ~/.cache/karen. Your permanent record is YOUR permanent record —
    // she doesn't pad it with her rehearsals, and she rehearsed plenty.
    let home = tempfile::tempdir().expect("temp home for hook state");
    let policies = fixture_policies();
    let mut cmd = Command::new(binary());
    cmd.env("HOME", home.path());
    cmd.arg("hook");
    cmd.arg("--adapter");
    cmd.arg("claude-classic");
    cmd.arg("--policies").arg(policies.path());
    cmd.args(args);
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(input.as_bytes()).expect("write stdin");
    }
    drop(child.stdin.take());

    let out = child.wait_with_output().expect("wait failed");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.code().unwrap_or(-1), stdout, stderr)
}

/// Validates that the hook protocol produces valid JSON envelope or {} on allow/fail-open
#[test]
fn hook_protocol_outputs_valid_json() {
    let input = r#"{"hook_event_name":"PreToolUse","session_id":"test","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
    let (code, stdout, _stderr) = run_hook(input, &[]);
    assert_eq!(code, 0);
    // Must be valid JSON (either {} or protocol object)
    serde_json::from_str::<serde_json::Value>(&stdout).expect("valid JSON output");
}

/// Bad JSON input triggers fail-open (empty object)
#[test]
fn hook_bad_json_is_fail_open() {
    let (code, stdout, _stderr) = run_hook("not json", &[]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "{}");
}

/// `SessionStart` with --part delivers rule chunks (structure test)
#[test]
fn hook_sessionstart_parts_emit_structured_output() {
    let session_id = format!("test-parts-{}", std::process::id());
    let input = format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{session_id}","source":"startup"}}"#
    );
    for part in 1..=5 {
        let (code, stdout, _stderr) = run_hook(&input, &["--part", &part.to_string()]);
        assert_eq!(code, 0);
        let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
        if part <= 4 {
            // Parts 1-4 must have hookSpecificOutput with additionalContext
            assert!(
                out.get("hookSpecificOutput").is_some(),
                "part {part} missing hookSpecificOutput: {stdout}"
            );
            assert_eq!(out["hookSpecificOutput"]["hookEventName"], "SessionStart");
            assert!(
                out["hookSpecificOutput"]["additionalContext"].is_string(),
                "part {part} missing additionalContext"
            );
        } else {
            // Part 5 (beyond packed count) emits empty object
            assert_eq!(
                out,
                serde_json::json!({}),
                "part {part} should be empty: {stdout}"
            );
        }
    }
}

/// Part beyond count emits empty object
#[test]
fn hook_sessionstart_part_beyond_count_is_empty() {
    let input =
        r#"{"hook_event_name":"SessionStart","session_id":"test-parts","source":"startup"}"#;
    let (code, stdout, _stderr) = run_hook(input, &["--part", "99"]);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(out, serde_json::json!({}));
}

/// validate command reports policy health as JSON envelope
#[test]
fn validate_command_reports_policy_health() {
    let policies = fixture_policies();
    let mut cmd = Command::new(binary());
    cmd.arg("validate");
    cmd.arg("--policies").arg(policies.path());
    cmd.arg("--json");
    let output = cmd.output().expect("failed to run validate");
    assert!(output.status.success());
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid json");
    assert_eq!(out["status"], "ok");
    assert!(out["data"]["gate_count"].is_number());
    assert!(out["data"]["rule_count"].is_number());
}

/// check command lists gates and rules in JSON
#[test]
fn check_command_lists_gates_and_rules() {
    let policies = fixture_policies();
    let mut cmd = Command::new(binary());
    cmd.arg("check");
    cmd.arg("--policies").arg(policies.path());
    cmd.arg("--json");
    let output = cmd.output().expect("failed to run check");
    assert!(output.status.success());
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid json");
    assert_eq!(out["status"], "ok");
    let gates = out["data"]["gates"].as_array().unwrap();
    // Should have at least her three fixture gates
    assert!(gates.len() >= 3);
    for gate in gates {
        assert!(gate["id"].is_string());
        assert!(gate["event"].is_string());
        assert!(gate["priority"].is_number());
        assert!(gate["action"].is_string());
    }
}

/// evaluate command performs dry-run and returns structured result
#[test]
fn evaluate_command_dry_runs() {
    let input = r#"{"hook_event_name":"PreToolUse","session_id":"test-eval","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
    let policies = fixture_policies();
    let mut cmd = Command::new(binary());
    cmd.arg("evaluate");
    cmd.arg("--policies").arg(policies.path());
    cmd.arg("--request");
    cmd.arg("-");
    cmd.arg("--json");
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let out_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(out_json["status"], "ok");
    assert!(out_json["data"]["action"].is_string());
    assert_eq!(out_json["data"]["dry_run"], true);
    assert!(out_json["data"]["injection_chars"].is_number());
    assert!(out_json["data"]["fired_policies"].is_array());
}

/// schema command lists all commands and types
#[test]
fn schema_command_lists_commands() {
    let mut cmd = Command::new(binary());
    cmd.arg("schema");
    cmd.arg("--json");
    let output = cmd.output().expect("failed to run schema");
    assert!(output.status.success());
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(out["status"], "ok");
    let commands: Vec<&str> = out["data"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(commands.contains(&"hook"));
    assert!(commands.contains(&"validate"));
    assert!(commands.contains(&"check"));
    assert!(commands.contains(&"evaluate"));
    assert!(commands.contains(&"schema"));
    // Check types are present
    assert!(out["data"]["types"]["events"].is_array());
    assert!(out["data"]["types"]["conditions"].is_array());
    assert!(out["data"]["types"]["actions"].is_array());
    assert!(out["data"]["types"]["exit_codes"].is_object());
}

/// schema with topic filters to one command
#[test]
fn schema_with_topic_returns_filtered() {
    let mut cmd = Command::new(binary());
    cmd.arg("schema");
    cmd.arg("hook");
    cmd.arg("--json");
    let output = cmd.output().expect("failed to run schema");
    assert!(output.status.success());
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(out["status"], "ok");
    let commands = out["data"]["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["name"], "hook");
}

/// Unknown schema topic returns error with exit code 2
#[test]
fn unknown_schema_topic_returns_error() {
    let mut cmd = Command::new(binary());
    cmd.arg("schema");
    cmd.arg("nonexistent");
    let output = cmd.output().expect("failed to run schema");
    assert!(!output.status.success());
    assert_eq!(output.status.code().unwrap(), 2);
}

/// JSON field filtering works on validate output
#[test]
fn validate_json_fields_filter() {
    let policies = fixture_policies();
    let mut cmd = Command::new(binary());
    cmd.arg("validate");
    cmd.arg("--policies").arg(policies.path());
    cmd.arg("--json");
    cmd.arg("--fields");
    cmd.arg("gate_count,rule_count");
    let output = cmd.output().expect("failed to run validate");
    assert!(output.status.success());
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid json");
    assert_eq!(out["status"], "ok");
    let data = &out["data"];
    // Only requested fields present
    assert!(data.get("gate_count").is_some());
    assert!(data.get("rule_count").is_some());
    assert!(data.get("gates").is_none());
    assert!(data.get("rules").is_none());
}

/// Unknown fields in --fields returns error
#[test]
fn validate_json_unknown_fields_errors() {
    let policies = fixture_policies();
    let mut cmd = Command::new(binary());
    cmd.arg("validate");
    cmd.arg("--policies").arg(policies.path());
    cmd.arg("--json");
    cmd.arg("--fields");
    cmd.arg("nonexistent_field");
    let output = cmd.output().expect("failed to run validate");
    assert!(!output.status.success());
    assert_eq!(output.status.code().unwrap(), 2);
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid json");
    assert_eq!(out["status"], "error");
    assert_eq!(out["error"]["code"], "INVALID_FIELDS");
}

/// Missing policies directory returns exit 78
#[test]
fn missing_policies_returns_config_error() {
    let bin = binary();
    let mut cmd = Command::new(&bin);
    cmd.arg("validate");
    cmd.arg("--policies");
    cmd.arg("/nonexistent/path");
    cmd.arg("--json");
    let output = cmd.output().expect("failed to run validate");
    assert!(!output.status.success());
    assert_eq!(output.status.code().unwrap(), 78);
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid json");
    assert_eq!(out["status"], "error");
    assert_eq!(out["error"]["code"], "POLICY_NOT_FOUND");
}

/// Engine loads gates from JSON and rules from markdown
#[test]
fn engine_loads_gates_and_rules_from_files() {
    let policies = fixture_policies();
    let mut cmd = Command::new(binary());
    cmd.arg("validate");
    cmd.arg("--policies").arg(policies.path());
    cmd.arg("--json");
    let output = cmd.output().expect("failed to run validate");
    assert!(output.status.success());
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid json");
    // Should have loaded her fixture gates and rules
    assert!(out["data"]["gate_count"].as_u64().unwrap() >= 3);
    assert!(out["data"]["rule_count"].as_u64().unwrap() >= 4);
}

/// Engine reads `fail_open` from config.toml
#[test]
fn engine_reads_fail_open_from_config() {
    let policies = fixture_policies();
    let mut cmd = Command::new(binary());
    cmd.arg("validate");
    cmd.arg("--policies").arg(policies.path());
    cmd.arg("--json");
    let output = cmd.output().expect("failed to run validate");
    assert!(output.status.success());
    let out: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid json");
    assert_eq!(out["data"]["config"]["fail_open"], true);
}

/// Hook protocol denies return permissionDecision: "deny" with reason
#[test]
fn hook_deny_returns_protocol_object() {
    // The Karen who writes a gate she knows will fire: the fixture force-push
    // gate guarantees the deny shape, so the shape gets proven, not sampled.
    let input = r#"{"hook_event_name":"PreToolUse","session_id":"test-deny-shape","tool_name":"Bash","tool_input":{"command":"git push --force origin main"}}"#;
    let (code, stdout, _stderr) = run_hook(input, &[]);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(
        out.get("hookSpecificOutput").is_some(),
        "expected a deny protocol object, got: {stdout}"
    );
    assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(
        out["hookSpecificOutput"]["permissionDecisionReason"].is_string(),
        "a deny without a reason is just a mood: {stdout}"
    );
}

/// `field_equals` falls back to top-level payload fields: a Stop gate blocks the
/// first stop of every turn and lets the continuation stop, so it never loops.
#[test]
fn stop_gate_reads_stop_hook_active() {
    let first = r#"{"hook_event_name":"Stop","session_id":"test-stop","stop_hook_active":false}"#;
    let (code, stdout, _) = run_hook(first, &[]);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        out["decision"], "block",
        "first stop must be blocked: {stdout}"
    );

    let again = r#"{"hook_event_name":"Stop","session_id":"test-stop","stop_hook_active":true}"#;
    let (code, stdout, _) = run_hook(again, &[]);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        out,
        serde_json::json!({}),
        "the continuation must be allowed to stop: {stdout}"
    );
}

/// `field_regex` reads the top-level `last_assistant_message`: a stop gate
/// holds the turn open until the closing statement carries the required text.
#[test]
fn stop_gate_reads_last_assistant_message() {
    let missing = r#"{"hook_event_name":"SubagentStop","session_id":"test-q","stop_hook_active":true,"last_assistant_message":"All done."}"#;
    let (code, stdout, _) = run_hook(missing, &[]);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        out["decision"], "block",
        "a closing message without [Q-7] must be blocked: {stdout}"
    );

    let asked = r#"{"hook_event_name":"SubagentStop","session_id":"test-q","stop_hook_active":true,"last_assistant_message":"[Q-7] Approve the pipeline? Context, options, recommendation."}"#;
    let (code, stdout, _) = run_hook(asked, &[]);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        out,
        serde_json::json!({}),
        "the re-asked question must let the turn end: {stdout}"
    );
}

/// Hook allows returns {} (no stdout content beyond empty object)
#[test]
fn hook_allow_returns_empty_object() {
    // Use a benign command that shouldn't match any gates
    let input = r#"{"hook_event_name":"PreToolUse","session_id":"test-allow-shape","tool_name":"Bash","tool_input":{"command":"echo hello"}}"#;
    let (code, stdout, _stderr) = run_hook(input, &[]);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    // Allow path: empty object
    assert_eq!(out, serde_json::json!({}));
}
