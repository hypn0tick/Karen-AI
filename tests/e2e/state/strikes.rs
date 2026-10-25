// tests/e2e/state/strikes.rs
// The Karen who tests strikes through the hook, because that's where counting
// actually matters. Evaluate is a rehearsal; this is the concert.

use crate::e2e::common::{
    DEFAULT_GATES, DEFAULT_RULES, TestEnv, assert_exit_code, assert_hook_deny,
    assert_hook_deny_once_first, assert_hook_warned_retry,
};
use serde_json::json;
use serial_test::serial;

/// Memory backend: every process starts with a clean ledger.
const MEMORY_CONFIG: &str = r#"# Personal Rules engine configuration.
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

/// File backend: the ledger survives between processes.
const FILE_CONFIG: &str = r#"# Personal Rules engine configuration.
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

fn jq_input(session_id: &str) -> serde_json::Value {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": session_id,
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    })
}

#[test]
#[serial]
fn karen_verifies_memory_backend_strikes_dont_persist() {
    // The Karen who proves amnesia has consequences — for you: with the
    // memory backend every hook process starts fresh, so every call is a
    // first strike. She forgets your apology, then demands it again.
    let env = TestEnv::with_config(MEMORY_CONFIG, DEFAULT_GATES, DEFAULT_RULES);
    let input = jq_input("strikes-memory-1").to_string();
    for _ in 0..2 {
        let (code, stdout, _stderr) = env.run_hook(&input, &[]);
        assert_exit_code(code, 0);
        assert_hook_deny(&stdout, Some("yq"));
        assert_hook_deny_once_first(&stdout, Some("yq"));
    }
}

#[test]
#[serial]
fn karen_verifies_file_backend_strikes_persist() {
    // The Karen who proves the file remembers across processes — she wrote
    // it down, and she'll quote it back to you verbatim.
    let env = TestEnv::with_config(FILE_CONFIG, DEFAULT_GATES, DEFAULT_RULES);
    let input = jq_input("strikes-file-1").to_string();
    let (code1, stdout1, _) = env.run_hook(&input, &[]);
    assert_exit_code(code1, 0);
    assert_hook_deny(&stdout1, Some("yq"));
    // The receipt exists on disk before she even checks it twice.
    let state_file = env
        .home_dir
        .join(".cache")
        .join("karen")
        .join("strikes-file-1.json");
    assert!(
        state_file.exists(),
        "file backend must persist the strike to {}: run `karen hook` with HOME set to {}",
        state_file.display(),
        env.home_dir.display()
    );
    let (code2, stdout2, _) = env.run_hook(&input, &[]);
    assert_exit_code(code2, 0);
    assert_hook_warned_retry(&stdout2, Some("jq"));
}

#[test]
#[serial]
fn karen_verifies_auto_backend_hook_uses_file() {
    // The Karen who verifies auto backend resolves to file in hook mode,
    // so strikes survive between the runtime's separate hook processes.
    let env = TestEnv::new();
    let input = jq_input("strikes-auto-hook").to_string();
    // First hook - first strike denies with coaching
    let (code1, stdout1, _) = env.run_hook(&input, &[]);
    assert_exit_code(code1, 0);
    assert_hook_deny_once_first(&stdout1, Some("yq"));
    // Second hook - second strike is a warned retry
    let (code2, stdout2, _) = env.run_hook(&input, &[]);
    assert_exit_code(code2, 0);
    assert_hook_warned_retry(&stdout2, Some("jq"));
}

#[test]
#[serial]
fn karen_verifies_strikes_reset_per_session() {
    // The Karen who verifies strikes are per-session: a new session is a
    // stranger with a clean record, and she checks the ID, not the face.
    let env = TestEnv::new();
    let input_a = jq_input("session-a").to_string();
    let input_b = jq_input("session-b").to_string();
    let (code_a, stdout_a, _) = env.run_hook(&input_a, &[]);
    assert_exit_code(code_a, 0);
    assert_hook_deny_once_first(&stdout_a, Some("yq"));
    // Session B - still a first strike, untouched by A's record
    let (code_b, stdout_b, _) = env.run_hook(&input_b, &[]);
    assert_exit_code(code_b, 0);
    assert_hook_deny_once_first(&stdout_b, Some("yq"));
}

#[test]
#[serial]
fn karen_verifies_max_strikes_custom() {
    // The Karen who honors a custom max_strikes: three strikes and the
    // baseball metaphor is over. She brought her own bat.
    let custom_gate = r#"{
  "id": "custom-strikes",
  "name": "Custom Strikes Test",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 10,
  "enabled": true,
  "tags": ["test"],
  "match": { "all": [{ "type": "command_regex", "pattern": "custom-strikes-test" }] },
  "action": {
    "type": "deny_once_then_warn",
    "first_reason": "strike 1",
    "second_reason": "strike 2"
  },
  "state": { "track": "session", "key": "custom-strikes", "max_strikes": 3 }
}"#;
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[crate::e2e::common::GateFixture {
            file: "custom-strikes.json",
            content: custom_gate,
        }],
        &[],
    );
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "custom-strikes-session",
        "tool_name": "Bash",
        "tool_input": { "command": "custom-strikes-test" }
    })
    .to_string();
    // Strikes 1 and 2 deny (max_strikes=3 means two denies before the warned retry)
    for _ in 0..2 {
        let (code, stdout, _stderr) = env.run_hook(&input, &[]);
        assert_exit_code(code, 0);
        assert_hook_deny(&stdout, Some("strike 1"));
    }
    // Strike 3 - the warned retry
    let (code3, stdout3, _) = env.run_hook(&input, &[]);
    assert_exit_code(code3, 0);
    assert_hook_warned_retry(&stdout3, Some("strike 2"));
}

#[test]
#[serial]
fn karen_verifies_injector_dedup_same_session() {
    // The Karen who verifies rule injection is deduplicated per session part.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "SessionStart",
        "session_id": "injector-dedup",
        "source": "startup"
    })
    .to_string();
    // First SessionStart part 1
    let (code1, stdout1, _) = env.run_hook(&input, &["--part", "1"]);
    assert_exit_code(code1, 0);
    let out1: serde_json::Value = serde_json::from_str(&stdout1).unwrap();
    assert!(out1["hookSpecificOutput"]["additionalContext"].is_string());

    // Second SessionStart part 1 (same session) - should be suppressed
    let (code2, stdout2, _) = env.run_hook(&input, &["--part", "1"]);
    assert_exit_code(code2, 0);
    let out2: serde_json::Value = serde_json::from_str(&stdout2).unwrap();
    assert_eq!(
        out2,
        serde_json::json!({}),
        "re-delivery of same part must be suppressed"
    );
}
