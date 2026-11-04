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

#[test]
#[serial]
fn karen_state_reset_on_compact_lets_a_nudge_fire_again() {
    // The Karen who repeats herself only when you have actually forgotten: a
    // nudge with reset_on_compact holds once, lets you pass, and after a
    // compaction holds once more. One without it stays quiet.
    const PER_CONTEXT: &str = r#"{
  "id": "per-context", "event": "PreToolUse", "priority": 10,
  "match": { "all": [{ "type": "command_regex", "pattern": "per-context-test" }] },
  "action": { "type": "deny_once_then_warn", "first_reason": "load it", "second_reason": "loaded" },
  "state": { "key": "per-context", "max_strikes": 2, "reset_on_compact": true }
}"#;
    const PER_SESSION: &str = r#"{
  "id": "per-session", "event": "PreToolUse", "priority": 10,
  "match": { "all": [{ "type": "command_regex", "pattern": "per-session-test" }] },
  "action": { "type": "deny_once_then_warn", "first_reason": "load it", "second_reason": "loaded" },
  "state": { "key": "per-session", "max_strikes": 2 }
}"#;
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[
            crate::e2e::common::GateFixture {
                file: "per-context.json",
                content: PER_CONTEXT,
            },
            crate::e2e::common::GateFixture {
                file: "per-session.json",
                content: PER_SESSION,
            },
        ],
        &[],
    );
    let call = |id: &str| {
        json!({
            "hook_event_name": "PreToolUse",
            "session_id": "compact-session",
            "tool_name": "Bash",
            "tool_input": { "command": format!("{id}-test") }
        })
        .to_string()
    };
    for id in ["per-context", "per-session"] {
        let (_, first, _) = env.run_hook(&call(id), &[]);
        assert_hook_deny(&first, Some("load it"));
        let (_, second, _) = env.run_hook(&call(id), &[]);
        assert_hook_warned_retry(&second, Some("loaded"));
    }
    let compact = json!({ "hook_event_name": "PostCompact", "session_id": "compact-session" });
    let (code, _, _) = env.run_hook(&compact.to_string(), &[]);
    assert_exit_code(code, 0);

    let (_, again, _) = env.run_hook(&call("per-context"), &[]);
    assert_hook_deny(&again, Some("load it"));
    let (_, kept, _) = env.run_hook(&call("per-session"), &[]);
    assert_hook_warned_retry(&kept, Some("loaded"));
}

#[test]
#[serial]
fn karen_counts_input_tracked_strikes_per_command() {
    let gate = r#"{ "id": "heredoc", "event": "PreToolUse", "tool": "Bash",
        "match": { "all": [ { "type": "command_contains", "value": "<<" } ] },
        "action": { "type": "deny_once_then_warn", "first_reason": "held once", "second_reason": "retry passes" },
        "state": { "track": "input", "key": "heredoc", "max_strikes": 2 } }"#;
    let env = TestEnv::with_config(
        FILE_CONFIG,
        &[crate::e2e::common::fixtures::GateFixture {
            file: "heredoc.json",
            content: gate,
        }],
        &[],
    );
    let call = |command: &str| {
        let input = json!({ "hook_event_name": "PreToolUse", "session_id": "input-track", "tool_name": "Bash", "tool_input": { "command": command } });
        let (code, stdout, _) = env.run_hook(&input.to_string(), &[]);
        assert_exit_code(code, 0);
        stdout
    };
    assert_hook_deny_once_first(&call("cat <<EOF one"), Some("held once"));
    assert_hook_warned_retry(&call("cat <<EOF one"), Some("retry passes"));
    // A new command is held again, which a session-tracked gate would not do.
    assert_hook_deny_once_first(&call("cat <<EOF two"), Some("held once"));

    std::fs::write(
        env.policies_dir.join("gates").join("heredoc.json"),
        gate.replace("\"input\"", "\"per-turn\""),
    )
    .unwrap();
    let (code, stdout, _) = env.run_karen(&["validate", "--json"]);
    assert_ne!(code, 0, "an unknown track is a load error: {stdout}");
}

#[test]
#[serial]
fn karen_ignores_a_commands_description_when_counting_input_strikes() {
    // The runtime's `description` is free text about a command, not part of
    // what runs: the same command retried with new wording is the same
    // command. Everything that changes what runs still counts.
    let shell_gate = r#"{ "id": "heredoc", "event": "PreToolUse", "tool": "Bash",
        "match": { "all": [ { "type": "command_contains", "value": "<<" } ] },
        "action": { "type": "deny_once_then_warn", "first_reason": "held once", "second_reason": "retry passes" },
        "state": { "track": "input", "key": "heredoc", "max_strikes": 2 } }"#;
    let tracker_gate = r#"{ "id": "tracker", "event": "PreToolUse", "tool": "create_issue",
        "match": { "all": [ { "type": "always" } ] },
        "action": { "type": "deny_once_then_warn", "first_reason": "filed once", "second_reason": "refile passes" },
        "state": { "track": "input", "key": "tracker", "max_strikes": 2 } }"#;
    let env = TestEnv::with_config(
        FILE_CONFIG,
        &[
            crate::e2e::common::fixtures::GateFixture {
                file: "heredoc.json",
                content: shell_gate,
            },
            crate::e2e::common::fixtures::GateFixture {
                file: "tracker.json",
                content: tracker_gate,
            },
        ],
        &[],
    );
    let call = |tool: &str, tool_input: serde_json::Value| {
        let input = json!({ "hook_event_name": "PreToolUse", "session_id": "input-description", "tool_name": tool, "tool_input": tool_input });
        let (code, stdout, _) = env.run_hook(&input.to_string(), &[]);
        assert_exit_code(code, 0);
        stdout
    };
    let shell = |command: &str, description: &str| {
        call(
            "Bash",
            json!({ "command": command, "description": description }),
        )
    };

    assert_hook_deny_once_first(&shell("cat <<EOF one", "write the file"), Some("held once"));
    // The same command in other words is the same command: it passes.
    assert_hook_warned_retry(
        &shell("cat <<EOF one", "put the text in the file"),
        Some("retry passes"),
    );
    // A different command is held, whatever it says it is for.
    assert_hook_deny_once_first(
        &shell("cat <<EOF two", "put the text in the file"),
        Some("held once"),
    );
    // A field that changes what runs still counts: the same command with a
    // longer timeout is a new call, held once.
    assert_hook_deny_once_first(
        &call(
            "Bash",
            json!({ "command": "cat <<EOF three", "timeout": 1000 }),
        ),
        Some("held once"),
    );
    assert_hook_deny_once_first(
        &call(
            "Bash",
            json!({ "command": "cat <<EOF three", "timeout": 2000 }),
        ),
        Some("held once"),
    );

    // Where `description` is the payload, not a note on a command, it stays
    // in the key: two issues that differ only in their description are two issues.
    assert_hook_deny_once_first(
        &call(
            "create_issue",
            json!({ "title": "bug", "description": "first" }),
        ),
        Some("filed once"),
    );
    assert_hook_deny_once_first(
        &call(
            "create_issue",
            json!({ "title": "bug", "description": "second" }),
        ),
        Some("filed once"),
    );
}

/// A deny-once gate on the `Bash`/`hold` command whose strike keys on `fields`.
fn key_fields_gate(id: &str, tool: &str, track: &str, fields: &str) -> &'static str {
    // Fixtures hold `&'static str`; a test process may leak a few lines of JSON.
    Box::leak(format!(
        r#"{{ "id": "{id}", "event": "PreToolUse", "tool": "{tool}",
        "match": {{ "all": [ {{ "type": "always" }} ] }},
        "action": {{ "type": "deny_once_then_warn", "first_reason": "held once", "second_reason": "retry passes" }},
        "state": {{ "track": "{track}", "key": "{id}", "key_fields": {fields} }} }}"#
    )
    .into_boxed_str())
}

fn key_fields_env(gate: &'static str) -> TestEnv {
    TestEnv::with_config(
        FILE_CONFIG,
        &[crate::e2e::common::fixtures::GateFixture {
            file: "key-fields.json",
            content: gate,
        }],
        &[],
    )
}

fn call(env: &TestEnv, session: &str, tool: &str, tool_input: serde_json::Value) -> String {
    let mut input =
        json!({ "hook_event_name": "PreToolUse", "session_id": session, "tool_name": tool });
    input["tool_input"] = tool_input;
    let (code, stdout, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code, 0);
    stdout
}

#[test]
#[serial]
fn karen_keys_a_strike_on_exactly_the_fields_the_gate_lists() {
    // The gate author knows what "the same input" is: here, the command. A
    // new description and a new timeout are the same kill, retried.
    let env = key_fields_env(key_fields_gate("kill", "Bash", "input", r#"["command"]"#));
    let run = |tool_input| call(&env, "key-fields-command", "Bash", tool_input);

    assert_hook_deny_once_first(
        &run(json!({ "command": "hcom kill zed", "description": "stop zed" })),
        Some("held once"),
    );
    assert_hook_warned_retry(
        &run(json!({ "command": "hcom kill zed", "description": "end zed", "timeout": 9000 })),
        Some("retry passes"),
    );
    // A different command is a different input.
    assert_hook_deny_once_first(
        &run(json!({ "command": "hcom kill abe", "description": "stop zed" })),
        Some("held once"),
    );
}

#[test]
#[serial]
fn karen_keys_a_non_shell_strike_on_a_listed_description() {
    // `description` is the payload of an issue tracker's call; listing it
    // keys the strike on it and nothing else, so the title may change.
    let env = key_fields_env(key_fields_gate(
        "tracker",
        "create_issue",
        "input",
        r#"["description"]"#,
    ));
    let run = |tool_input| call(&env, "key-fields-description", "create_issue", tool_input);

    assert_hook_deny_once_first(
        &run(json!({ "title": "bug", "description": "first" })),
        Some("held once"),
    );
    assert_hook_warned_retry(
        &run(json!({ "title": "another title", "description": "first" })),
        Some("retry passes"),
    );
    assert_hook_deny_once_first(
        &run(json!({ "title": "bug", "description": "second" })),
        Some("held once"),
    );
}

#[test]
#[serial]
fn karen_hashes_a_missing_listed_field_apart_from_an_empty_one() {
    // Absence is never a value: a call that left `note` out and one that sent
    // `note: ""` or `note: null` are three different inputs.
    let env = key_fields_env(key_fields_gate("note", "Bash", "input", r#"["note"]"#));
    let run = |tool_input| call(&env, "key-fields-absent", "Bash", tool_input);

    assert_hook_deny_once_first(&run(json!({ "command": "x" })), Some("held once"));
    assert_hook_deny_once_first(
        &run(json!({ "command": "x", "note": "" })),
        Some("held once"),
    );
    assert_hook_deny_once_first(
        &run(json!({ "command": "x", "note": null })),
        Some("held once"),
    );
    // Each of the three is remembered as its own input.
    assert_hook_warned_retry(&run(json!({ "command": "y" })), Some("retry passes"));
    assert_hook_warned_retry(
        &run(json!({ "command": "y", "note": "" })),
        Some("retry passes"),
    );
}

#[test]
#[serial]
fn karen_resolves_a_listed_path_through_nested_tool_input() {
    let env = key_fields_env(key_fields_gate(
        "nested",
        "deploy",
        "input",
        r#"["options.target"]"#,
    ));
    let run = |tool_input| call(&env, "key-fields-nested", "deploy", tool_input);

    assert_hook_deny_once_first(
        &run(json!({ "options": { "target": "prod", "dry": true } })),
        Some("held once"),
    );
    assert_hook_warned_retry(
        &run(json!({ "options": { "target": "prod", "dry": false } })),
        Some("retry passes"),
    );
    assert_hook_deny_once_first(
        &run(json!({ "options": { "target": "staging" } })),
        Some("held once"),
    );
}

#[test]
#[serial]
fn karen_validate_warns_about_key_fields_without_input_tracking() {
    let stray = key_fields_gate("stray", "Bash", "session", r#"["command"]"#);
    let fine = key_fields_gate("fine", "Bash", "input", r#"["command"]"#);
    let env = TestEnv::with_config(
        FILE_CONFIG,
        &[
            crate::e2e::common::fixtures::GateFixture {
                file: "stray.json",
                content: stray,
            },
            crate::e2e::common::fixtures::GateFixture {
                file: "fine.json",
                content: fine,
            },
        ],
        &[],
    );
    let (code, stdout, _) = env.run_karen(&["validate", "--json"]);
    // A warning does not fail the policy.
    assert_exit_code(code, 0);
    let data = crate::e2e::common::assert_json_ok(&stdout);
    let issues = data["issues"].as_array().expect("issues array");
    assert!(
        issues.iter().any(|i| {
            i["kind"] == "warning"
                && i["message"].as_str().is_some_and(|m| {
                    m.contains("`stray`") && m.contains("key_fields") && m.contains("track")
                })
        }),
        "validate must name the gate whose key_fields is dead text: {issues:?}"
    );
    // The input-tracked gate's list is in force, so it is not named.
    assert!(
        !issues
            .iter()
            .any(|i| i["message"].as_str().is_some_and(|m| m.contains("`fine`"))),
        "a key_fields on track \"input\" is not dead text: {issues:?}"
    );
}

#[test]
#[serial]
fn karen_refuses_an_empty_key_fields_and_keeps_every_other_gate() {
    let empty = key_fields_gate("hollow", "Bash", "input", "[]");
    let blank = key_fields_gate("blank", "Bash", "input", r#"["command", " "]"#);
    let sturdy = r#"{ "id": "sturdy", "event": "PreToolUse", "tool": "Bash",
        "match": { "all": [ { "type": "command_contains", "value": "still-enforced" } ] },
        "action": { "type": "deny", "reason": "sturdy holds" } }"#;
    let env = TestEnv::with_config(
        FILE_CONFIG,
        &[
            crate::e2e::common::fixtures::GateFixture {
                file: "hollow.json",
                content: empty,
            },
            crate::e2e::common::fixtures::GateFixture {
                file: "blank.json",
                content: blank,
            },
            crate::e2e::common::fixtures::GateFixture {
                file: "sturdy.json",
                content: sturdy,
            },
        ],
        &[],
    );
    let (code, stdout, _) = env.run_karen(&["validate", "--json"]);
    assert_ne!(code, 0, "an empty key_fields is a load error: {stdout}");
    assert!(
        stdout.contains("hollow"),
        "validate names the gate: {stdout}"
    );
    assert!(
        stdout.contains("blank"),
        "validate names the gate: {stdout}"
    );

    // The refused gates stay out; the rest of the policy is still enforced.
    let held = call(
        &env,
        "key-fields-refused",
        "Bash",
        json!({ "command": "still-enforced" }),
    );
    assert_hook_deny(&held, Some("sturdy holds"));
    let quiet = call(
        &env,
        "key-fields-refused",
        "Bash",
        json!({ "command": "echo ok" }),
    );
    assert!(
        !quiet.contains("held once"),
        "a refused gate never fires: {quiet}"
    );
}
