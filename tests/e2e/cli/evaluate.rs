// tests/e2e/cli/evaluate.rs
// The Karen who evaluates evaluate because dry runs must be bone dry.

use crate::e2e::common::{
    DEFAULT_GATES, DEFAULT_RULES, TestEnv, assert_evaluate_action, assert_exit_code,
    assert_fields_filtered, assert_json_error, assert_json_ok,
};
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn karen_evaluate_allow_benign_command() {
    // The Karen who evaluates a benign command and expects allow.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-allow",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "allow");
    assert_eq!(data["dry_run"], true);
    assert_eq!(data["injection_chars"], 0);
}

#[test]
#[serial]
fn karen_evaluate_deny_commit_attribution() {
    // The Karen who evaluates a commit with attribution and expects deny.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-deny",
        "tool_name": "Bash",
        "tool_input": { "command": "git commit -m \"x 🤖\"" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "deny");
    assert!(data["reason"].as_str().unwrap().contains("attribution"));
    assert!(
        data["fired_policies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap() == "commit-attribution")
    );
}

#[test]
#[serial]
fn karen_evaluate_modify_head_command() {
    // The Karen who evaluates head command and expects modify to Select-Object.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-modify",
        "tool_name": "Bash",
        "tool_input": { "command": "head -5 file.txt" }
    });
    let (code, stdout, _stderr) =
        env.run_evaluate_stdin(&input.to_string(), &["--json", "--include-content"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "modify");
    assert!(data["modified_input"].is_object());
    let modified = &data["modified_input"];
    assert!(
        modified["command"]
            .as_str()
            .unwrap()
            .contains("Select-Object -First 5")
    );
}

#[test]
#[serial]
fn karen_evaluate_modify_head_pipeline() {
    // The Karen who evaluates head in pipeline and expects modify.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-modify-pipeline",
        "tool_name": "Bash",
        "tool_input": { "command": "cat file.txt | head -n 10" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "modify");
    let modified = &data["modified_input"];
    assert!(
        modified["command"]
            .as_str()
            .unwrap()
            .contains("Select-Object -First 10")
    );
}

#[test]
#[serial]
fn karen_evaluate_deny_once_then_warn_first_strike() {
    // The Karen who evaluates jq on yaml and expects first-strike deny.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-deny-once-1",
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "deny");
    let reason = data["reason"].as_str().unwrap();
    assert!(reason.contains("yq"), "first strike must mention yq");
    assert!(
        data["fired_policies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap() == "jq-yaml-redirect")
    );
}

#[test]
#[serial]
fn karen_evaluate_is_read_only_strikes_not_consumed() {
    // The Karen who proves the rehearsal eats nothing: the same first
    // strike, twice, and the ledger stays clean. A dry run that consumes
    // your only warning is sabotage with a smile.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-readonly-1",
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    for _ in 0..2 {
        let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
        assert_exit_code(code, 0);
        let data = assert_json_ok(&stdout);
        assert_evaluate_action(&data, "deny");
        let reason = data["reason"].as_str().unwrap();
        assert!(
            reason.contains("yq"),
            "preview must stay on the first strike: {reason}"
        );
    }
    // And the state directory holds no session file — nothing was written down.
    let state_dir = env.home_dir.join(".cache").join("karen");
    let files: Vec<_> = std::fs::read_dir(&state_dir)
        .map(|entries| entries.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default();
    assert!(
        files.is_empty(),
        "evaluate must not persist state, found: {:?}",
        files
            .iter()
            .map(std::fs::DirEntry::file_name)
            .collect::<Vec<_>>()
    );
}

#[test]
#[serial]
fn karen_evaluate_web_search_read_only() {
    // The Karen who previews WebSearch twice and denies twice — the
    // Brave-Exa lecture is on the house, the strike is not.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-web-readonly",
        "tool_name": "WebSearch",
        "tool_input": { "query": "rust" }
    });
    for _ in 0..2 {
        let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
        assert_exit_code(code, 0);
        let data = assert_json_ok(&stdout);
        assert_evaluate_action(&data, "deny");
        let reason = data["reason"].as_str().unwrap();
        assert!(reason.contains("Brave"));
    }
}

#[test]
#[serial]
fn karen_evaluate_cargo_quality_read_only() {
    // The Karen who previews git push twice and demands cargo fmt both
    // times. Repetition is free when nothing is recorded.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-cargo-readonly",
        "tool_name": "Bash",
        "tool_input": { "command": "git push" }
    });
    for _ in 0..2 {
        let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
        assert_exit_code(code, 0);
        let data = assert_json_ok(&stdout);
        assert_evaluate_action(&data, "deny");
        let reason = data["reason"].as_str().unwrap();
        assert!(reason.contains("cargo fmt"));
    }
}

#[test]
#[serial]
fn karen_evaluate_backticks_read_only() {
    // The Karen who previews the unbackticked write twice and lectures
    // twice. The formatting sermon is infinite; the strike counter is not.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-backticks-readonly",
        "tool_name": "Write",
        "tool_input": { "file_path": "test.rs", "content": "fn main() { cargo build(); }" }
    });
    for _ in 0..2 {
        let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
        assert_exit_code(code, 0);
        let data = assert_json_ok(&stdout);
        assert_evaluate_action(&data, "deny");
        let reason = data["reason"].as_str().unwrap();
        assert!(reason.contains("backticks"));
    }
}

#[test]
#[serial]
fn karen_evaluate_json_fields_filter() {
    // The Karen who filters evaluate output fields.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-eval-fields",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(
        &input.to_string(),
        &["--json", "--fields", "action,reason,dry_run"],
    );
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_fields_filtered(&data, &["action", "reason", "dry_run"]);
}

#[test]
#[serial]
fn karen_evaluate_conflicting_flags_errors() {
    // The Karen who rejects conflicting --request and flag mode.
    let env = TestEnv::new();
    let input = json!({ "hook_event_name": "PreToolUse", "session_id": "x" });
    let (code, stdout, _stderr) =
        env.run_evaluate_stdin(&input.to_string(), &["--json", "--event", "PreToolUse"]);
    assert_exit_code(code, 2);
    assert_json_error(&stdout, "INVALID_INPUT");
}

#[test]
#[serial]
fn karen_evaluate_missing_event_errors() {
    // The Karen who rejects missing event.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["evaluate", "--json"]);
    assert_exit_code(code, 2);
    assert_json_error(&stdout, "INVALID_INPUT");
}

#[test]
#[serial]
fn karen_evaluate_request_not_found_errors() {
    // The Karen who rejects nonexistent request file.
    let env = TestEnv::new();
    let (code, stdout, _stderr) =
        env.run_karen(&["evaluate", "--json", "--request", "/nonexistent/file.json"]);
    assert_exit_code(code, 3);
    assert_json_error(&stdout, "REQUEST_NOT_FOUND");
}

#[test]
#[serial]
fn karen_evaluate_request_bad_json_errors() {
    // The Karen who rejects invalid JSON in request file.
    let env = TestEnv::new();
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), "not json").unwrap();
    let (code, stdout, _stderr) = env.run_karen(&[
        "evaluate",
        "--json",
        "--request",
        temp.path().to_str().unwrap(),
    ]);
    assert_exit_code(code, 2);
    assert_json_error(&stdout, "INVALID_JSON");
}

#[test]
#[serial]
fn karen_evaluate_control_chars_in_flags_errors() {
    // The Karen who rejects control characters in flag values.
    // Test via stdin request to avoid OS-level nul byte rejection in argv.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse\x00",
        "session_id": "test-ctrl",
        "tool_name": "Bash",
        "tool_input": { "command": "echo" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 2);
    // The error code for invalid request payload is INVALID_REQUEST
    assert_json_error(&stdout, "INVALID_REQUEST");
}

#[test]
#[serial]
fn karen_evaluate_never_writes_state() {
    // The Karen who verifies evaluate never persists state (MemoryStore).
    let config = r#"
[engine]
fail_open = true
log_level = "warn"
timeout_ms = 5000

[policy]
rules_dir = "rules"
gates_dir = "gates"
validate_on_load = true

[state]
backend = "memory"
file_dir = "~/.cache/karen"
"#;
    let env = TestEnv::with_config(config, DEFAULT_GATES, DEFAULT_RULES);
    let session_id = "test-eval-state";
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": session_id,
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    // First evaluate - should be first strike
    let (code1, stdout1, _) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code1, 0);
    let data1 = assert_json_ok(&stdout1);
    assert_evaluate_action(&data1, "deny");

    // Second evaluate with SAME session - should ALSO be first strike (MemoryStore doesn't persist)
    let (code2, stdout2, _) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code2, 0);
    let data2 = assert_json_ok(&stdout2);
    assert_evaluate_action(&data2, "deny"); // Still first strike because MemoryStore is fresh each time
}
