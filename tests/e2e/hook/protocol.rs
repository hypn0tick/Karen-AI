// tests/e2e/hook/protocol.rs
// The Karen who tests hook protocols because she doesn't trust anything that speaks JSON without verification.

use crate::e2e::common::{
    TestEnv, assert_exit_code, assert_hook_allow, assert_hook_deny, assert_hook_deny_once_first,
    assert_hook_modify, assert_hook_warned_retry, assert_valid_json,
};
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn karen_verifies_hook_protocol_outputs_valid_json() {
    // The Karen who knows every hook output must be valid JSON.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-hook-protocol",
        "tool_name": "Bash",
        "tool_input": { "command": "ls" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code, 0);
    let _ = assert_valid_json(&stdout);
}

#[test]
#[serial]
fn karen_verifies_hook_bad_json_is_fail_open() {
    // The Karen who implemented fail-open once had a hook crash during a demo.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_hook("not json", &[]);
    assert_exit_code(code, 0);
    assert_hook_allow(&stdout);
}

#[test]
#[serial]
fn karen_verifies_allow_returns_empty_object() {
    // The Karen who verifies allow returns empty object per protocol.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-hook-allow",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code, 0);
    assert_hook_allow(&stdout);
}

#[test]
#[serial]
fn karen_verifies_deny_returns_protocol_object() {
    // The Karen who knows a deny must have permissionDecision and a reason.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-hook-deny",
        "tool_name": "Bash",
        "tool_input": { "command": "git commit -m \"x 🤖\"" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code, 0);
    assert_hook_deny(&stdout, Some("attribution"));
}

#[test]
#[serial]
fn karen_verifies_deny_once_then_warn_first_strike_denies_and_coaches() {
    // The Karen who denies AND explains: a first strike that only lectures
    // without blocking is a suggestion wearing a badge.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-hook-deny-once-1",
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code, 0);
    assert_hook_deny(&stdout, Some("yq"));
    assert_hook_deny_once_first(&stdout, Some("yq"));
}

#[test]
#[serial]
fn karen_verifies_deny_once_then_warn_second_strike_is_warned_allow() {
    // The Karen who allows the retry — loudly, with the nudge attached,
    // so the agent knows exactly which shortcut was tolerated.
    let env = TestEnv::new();
    let session_id = "test-hook-deny-once-2";
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": session_id,
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    // First strike
    env.run_hook(&input.to_string(), &[]);
    // Second strike
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code, 0);
    assert_hook_warned_retry(&stdout, Some("jq"));
}

#[test]
#[serial]
fn karen_verifies_modify_returns_modified_input() {
    // The Karen who verifies modify action returns modifiedInput in protocol.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-hook-modify",
        "tool_name": "Bash",
        "tool_input": { "command": "head -5 file.txt" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code, 0);
    assert_hook_modify(&stdout, Some("Select-Object -First 5"));
}

#[test]
#[serial]
fn karen_verifies_claude_adapter_protocol_shape() {
    // The Karen who verifies the claude adapter (current) produces same protocol shape.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test-hook-claude-adapter",
        "tool_name": "Bash",
        "tool_input": { "command": "git commit -m \"x 🤖\"" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &["--adapter", "claude"]);
    assert_exit_code(code, 0);
    assert_hook_deny(&stdout, Some("attribution"));
}
