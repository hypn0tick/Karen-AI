// tests/e2e/adapters/claude_classic.rs
// The Karen who tests the classic adapter because classic never goes out of style.

use crate::e2e::common::{
    TestEnv, assert_exit_code, assert_hook_allow, assert_hook_deny, assert_hook_modify,
    assert_hook_sessionstart_part,
};
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn karen_verifies_claude_classic_adapter_deny_shape() {
    // The Karen who denies with the full protocol shape — reason and all.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "adapter-classic-deny",
        "tool_name": "Bash",
        "tool_input": { "command": "git commit -m \"x 🤖\"" }
    });
    let (code, stdout, _stderr) =
        env.run_hook(&input.to_string(), &["--adapter", "claude-classic"]);
    assert_exit_code(code, 0);
    assert_hook_deny(&stdout, Some("attribution"));
}

#[test]
#[serial]
fn karen_verifies_claude_classic_adapter_allow_shape() {
    // The Karen who allows benign commands with protocol silence: {}.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "adapter-classic-allow",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" }
    });
    let (code, stdout, _stderr) =
        env.run_hook(&input.to_string(), &["--adapter", "claude-classic"]);
    assert_exit_code(code, 0);
    assert_hook_allow(&stdout);
}

#[test]
#[serial]
fn karen_verifies_claude_classic_adapter_modify_shape() {
    // The Karen who modifies and hands back the rewritten command where the
    // runtime can actually read it.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "adapter-classic-modify",
        "tool_name": "Bash",
        "tool_input": { "command": "head -5 file.txt" }
    });
    let (code, stdout, _stderr) =
        env.run_hook(&input.to_string(), &["--adapter", "claude-classic"]);
    assert_exit_code(code, 0);
    assert_hook_modify(&stdout, Some("Select-Object -First 5"));
}

#[test]
#[serial]
fn karen_verifies_claude_classic_adapter_sessionstart_shape() {
    // The Karen who delivers rules on SessionStart through the classic
    // adapter, because context injection is the whole point of her job.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "SessionStart",
        "session_id": "adapter-classic-session",
        "source": "startup"
    });
    let (code, stdout, _stderr) = env.run_hook(
        &input.to_string(),
        &["--adapter", "claude-classic", "--part", "1"],
    );
    assert_exit_code(code, 0);
    assert_hook_sessionstart_part(&stdout, 1);
}

#[test]
#[serial]
fn karen_verifies_claude_classic_adapter_provider_error_event() {
    // The Karen who hears the provider's own failure as an event: a gate
    // written for `ProviderError` applies, matched on a fact the payload
    // itself carries (no transcript read needed to know who is speaking).
    let gate = r#"{
  "id": "provider-error-event",
  "description": "Fires on the provider's own failure event.",
  "event": "ProviderError",
  "match": { "all": [ { "type": "field_equals", "field": "facts.session_id", "value": "adapter-provider-error" } ] },
  "action": { "type": "deny", "reason": "the provider event reached a gate" }
}"#;
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[crate::e2e::common::GateFixture {
            file: "provider-error-event.json",
            content: gate,
        }],
        &[],
    );
    let input = json!({
        "hook_event_name": "ProviderError",
        "session_id": "adapter-provider-error",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" }
    });
    let (code, stdout, _stderr) =
        env.run_hook(&input.to_string(), &["--adapter", "claude-classic"]);
    assert_exit_code(code, 0);
    // No tool call to hold: the block rides the generic shape, as Stop's does.
    let out: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON output");
    assert_eq!(
        out["decision"], "block",
        "a deny on ProviderError blocks: {stdout}"
    );
    assert!(
        out["reason"]
            .as_str()
            .is_some_and(|r| r.contains("provider event reached a gate")),
        "the gate's reason rides the block: {stdout}"
    );
}
