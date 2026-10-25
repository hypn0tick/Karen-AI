// tests/e2e/adapters/claude_current.rs
// The Karen who tests the current adapter because staying current is the only way.

use crate::e2e::common::{
    TestEnv, assert_exit_code, assert_hook_allow, assert_hook_deny, assert_hook_modify,
};
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn karen_verifies_claude_current_adapter_deny_shape() {
    // The Karen who denies with the full protocol shape, current dialect.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "adapter-current-deny",
        "tool_name": "Bash",
        "tool_input": { "command": "git commit -m \"x 🤖\"" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &["--adapter", "claude"]);
    assert_exit_code(code, 0);
    assert_hook_deny(&stdout, Some("attribution"));
}

#[test]
#[serial]
fn karen_verifies_claude_current_adapter_allow_shape() {
    // The Karen who allows benign commands with protocol silence: {}.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "adapter-current-allow",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &["--adapter", "claude"]);
    assert_exit_code(code, 0);
    assert_hook_allow(&stdout);
}

#[test]
#[serial]
fn karen_verifies_claude_current_adapter_modify_shape() {
    // The Karen who modifies through the current adapter and hands back the
    // rewritten command at the top level, where the runtime reads it.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "adapter-current-modify",
        "tool_name": "Bash",
        "tool_input": { "command": "head -5 file.txt" }
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &["--adapter", "claude"]);
    assert_exit_code(code, 0);
    assert_hook_modify(&stdout, Some("Select-Object -First 5"));
}
