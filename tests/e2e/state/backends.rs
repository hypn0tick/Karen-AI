// tests/e2e/state/backends.rs
// The Karen who tests backends because state is the memory of her enforcement.

use crate::e2e::common::TestEnv;
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn karen_verifies_file_backend_creates_state_dir() {
    // The Karen who verifies file backend creates the state directory.
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
backend = "file"
file_dir = "~/.cache/karen"
"#;
    let env = TestEnv::with_config(config, &[], &[]);
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "backend-file-dir",
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    // State dir should exist
    let state_dir = env.home_dir.join(".cache").join("karen");
    assert!(
        state_dir.exists(),
        "file backend must create state directory"
    );
}

#[test]
#[serial]
fn karen_verifies_memory_backend_no_state_dir() {
    // The Karen who verifies memory backend doesn't create state files.
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
    let env = TestEnv::with_config(config, &[], &[]);
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "backend-memory",
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    // Memory backend never touches disk. The Karen who proves it: no state
    // directory, no session file, no paper trail. What happens in memory
    // stays in memory — and then doesn't.
    let state_dir = env.home_dir.join(".cache").join("karen");
    assert!(
        !state_dir.exists(),
        "memory backend must not create a state directory: {}",
        state_dir.display()
    );
}
