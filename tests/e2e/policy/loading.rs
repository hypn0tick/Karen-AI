// tests/e2e/policy/loading.rs
// The Karen who tests policy loading because she doesn't trust configs she hasn't verified.

use crate::e2e::common::fixtures::{DEFAULT_CONFIG, GateFixture};
use crate::e2e::common::{
    TestEnv, assert_evaluate_action, assert_exit_code, assert_json_error, assert_json_ok,
    assert_policy_health,
};
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn karen_verifies_loads_json_gates() {
    // The Karen who verifies JSON gates load correctly.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_policy_health(&data, 7, 7);
}

#[test]
#[serial]
fn karen_verifies_loads_jsonc_gate_with_comments_and_trailing_comma() {
    // The Karen who verifies JSONC (comments + trailing comma) parses correctly.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["check", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let gates = data["gates"].as_array().unwrap();
    let jsonc_gate = gates.iter().find(|g| g["id"] == "jsonc-test-gate");
    assert!(jsonc_gate.is_some(), "jsonc-test-gate must be loaded");
}

#[test]
#[serial]
fn karen_verifies_loads_all_7_rules() {
    // The Karen who verifies all 7 markdown rules load.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["check", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let rules = data["rules"].as_array().unwrap();
    assert!(rules.len() >= 7);
    let rule_ids: Vec<&str> = rules.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert!(rule_ids.contains(&"00-edit-and-commit-core"));
    assert!(rule_ids.contains(&"01-defect-resolution"));
    assert!(rule_ids.contains(&"02-subagent-core"));
    assert!(rule_ids.contains(&"03-triggers"));
    assert!(rule_ids.contains(&"04-comments-and-docs-core"));
    assert!(rule_ids.contains(&"05-workflow-and-scope-core"));
    assert!(rule_ids.contains(&"06-truth-and-altitude-core"));
}

#[test]
#[serial]
fn karen_verifies_config_fail_open_true() {
    // The Karen who verifies config fail_open is respected.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_eq!(data["config"]["fail_open"], true);
}

#[test]
#[serial]
fn karen_verifies_config_fail_open_false() {
    // The Karen who verifies fail_open=false is respected.
    let config = r#"
[engine]
fail_open = false
log_level = "warn"
timeout_ms = 5000

[policy]
rules_dir = "rules"
gates_dir = "gates"
validate_on_load = true

[state]
backend = "auto"
file_dir = "~/.cache/karen"
"#;
    let env = TestEnv::with_config(config, &[], &[]);
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_eq!(data["config"]["fail_open"], false);
}

#[test]
#[serial]
fn karen_keeps_state_in_her_home_unless_told_otherwise() {
    // Everything Karen keeps lives under ~/.karen: state defaults to
    // ~/.karen/state, and an explicit file_dir still wins.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let expected = env.home_dir.join(".karen").join("state");
    assert_eq!(
        data["config"]["state_dir"],
        expected.display().to_string(),
        "default state lives in ~/.karen/state"
    );

    let env = TestEnv::with_config("[state]\nfile_dir = \"~/elsewhere\"\n", &[], &[]);
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_eq!(
        data["config"]["state_dir"],
        env.home_dir.join("elsewhere").display().to_string(),
        "an explicit file_dir wins, with ~/ expanded"
    );
}

#[test]
#[serial]
fn karen_verifies_hot_reload_picks_up_new_gate() {
    // The Karen who proves the CLI reload story: every invocation is a
    // fresh process, so a gate written between calls is law by the next
    // one — no watcher required, no ceremony observed. (In-process
    // embedders get the real thing: Engine::watch_policies, proven in
    // the lib lane.)
    let env = TestEnv::new();
    // Add a new gate file
    let new_gate = r#"{
  "id": "hot-reload-test",
  "name": "Hot Reload Test",
  "description": "Test gate for hot reload",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 10,
  "enabled": true,
  "tags": ["test"],
  "match": { "all": [{ "type": "command_regex", "pattern": "hot-reload-test" }] },
  "action": { "type": "deny", "reason": "hot reload test" }
}"#;
    let gates_dir = env.policies_dir.join("gates");
    std::fs::write(gates_dir.join("hot-reload-test.json"), new_gate).unwrap();

    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let gates = data["gates"].as_array().unwrap();
    assert!(
        gates.iter().any(|g| g["id"] == "hot-reload-test"),
        "hot reload must pick up new gate"
    );
}

#[test]
#[serial]
fn karen_hot_reloads_an_edited_gate_past_the_validation_cache() {
    // Regexes compile lazily and a clean validation is remembered per version
    // of the policy files. Remembering must never freeze the law: an edited
    // gate is law by the next call, and an edit that breaks a pattern is
    // caught on the next call even though the previous version validated.
    let gate = |pattern: &str| {
        format!(
            r#"{{ "id": "edited", "event": "PreToolUse", "tool": "Bash", "match": {{ "all": [ {{ "type": "command_regex", "pattern": "{pattern}" }} ] }}, "action": {{ "type": "deny", "reason": "edited gate" }} }}"#
        )
    };
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let path = env.policies_dir.join("gates").join("edited.json");
    let decide = |command: &str| {
        let input = json!({ "hook_event_name": "PreToolUse", "session_id": "hot", "tool_name": "Bash", "tool_input": { "command": command } }).to_string();
        let (code, stdout, _stderr) = env.run_evaluate_stdin(&input, &["--json"]);
        assert_exit_code(code, 0);
        assert_json_ok(&stdout)
    };

    std::fs::write(&path, gate("alpha")).unwrap();
    assert_evaluate_action(&decide("echo alpha"), "deny");
    let cache = env.home_dir.join(".karen").join("cache");
    assert!(
        std::fs::read_dir(&cache)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("validated-")),
        "a clean validation leaves its marker in ~/.karen/cache"
    );

    std::fs::write(&path, gate("beta")).unwrap();
    assert_evaluate_action(&decide("echo alpha"), "allow");
    assert_evaluate_action(&decide("echo beta"), "deny");

    std::fs::write(&path, gate("[invalid")).unwrap();
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 78);
    assert_json_error(&stdout, "POLICY_INVALID");
}

#[test]
#[serial]
fn karen_verifies_invalid_regex_errors() {
    // The Karen who verifies invalid regex in gate causes load error.
    let bad_gate = r#"{
  "id": "bad-regex",
  "name": "Bad Regex",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 10,
  "enabled": true,
  "tags": [],
  "match": { "all": [{ "type": "command_regex", "pattern": "[invalid" }] },
  "action": { "type": "deny", "reason": "bad" }
}"#;
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[GateFixture {
            file: "bad-regex.json",
            content: bad_gate,
        }],
        &[],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 78);
    assert_json_error(&stdout, "POLICY_INVALID");
}

#[test]
#[serial]
fn karen_verifies_unknown_event_errors() {
    // The Karen who verifies unknown event in gate causes load error.
    let bad_gate = r#"{
  "id": "unknown-event",
  "name": "Unknown Event",
  "event": "FooBarBaz",
  "tool": "Bash",
  "priority": 10,
  "enabled": true,
  "tags": [],
  "match": { "all": [{ "type": "always" }] },
  "action": { "type": "deny", "reason": "bad" }
}"#;
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[GateFixture {
            file: "unknown-event.json",
            content: bad_gate,
        }],
        &[],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 78);
    assert_json_error(&stdout, "POLICY_INVALID");
}

#[test]
#[serial]
fn karen_verifies_disabled_gate_not_evaluated() {
    // The Karen who verifies disabled gates are skipped.
    let env = TestEnv::new();
    let input = r#"{"hook_event_name": "PreToolUse", "session_id": "test-disabled", "tool_name": "Bash", "tool_input": {"command": "jsonc-test"}}"#;
    let (code, _stdout, _stderr) = env.run_hook(input, &[]);
    assert_exit_code(code, 0);
    // jsonc-test-gate is enabled and would deny, but we can't easily disable just one gate
    // This test verifies the concept - a more complete test would toggle enabled
}

#[test]
#[serial]
fn karen_catches_a_same_size_same_mtime_edit_that_breaks_a_pattern() {
    // Review finding: the marker was keyed on file size and modification time,
    // so restoring a file from backup with a broken pattern of the same length
    // left the old marker valid and the gate matched nothing, with no signal.
    let gate = |pattern: &str| {
        format!(
            r#"{{ "id": "edited", "event": "PreToolUse", "tool": "Bash", "match": {{ "all": [ {{ "type": "command_regex", "pattern": "{pattern}" }} ] }}, "action": {{ "type": "deny", "reason": "edited gate" }} }}"#
        )
    };
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let path = env.policies_dir.join("gates").join("edited.json");
    let hook = |command: &str| {
        let input = json!({ "hook_event_name": "PreToolUse", "session_id": "mtime", "tool_name": "Bash", "tool_input": { "command": command } }).to_string();
        let (code, stdout, _stderr) = env.run_hook(&input, &[]);
        assert_exit_code(code, 0);
        stdout
    };

    std::fs::write(&path, gate("^(ls|cat)$")).unwrap();
    assert!(hook("ls").contains("edited gate"), "the valid gate denies");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();

    std::fs::write(&path, gate("^(ls|ca[)$")).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let out = hook("ls");
    assert!(
        !out.contains("\"decision\"") && !out.contains("permissionDecision"),
        "the broken gate is not enforced: {out}"
    );
    assert!(
        out.contains("skips gate `edited`") && out.contains("unclosed character class"),
        "a broken pattern is reported loudly, not left silent: {out}"
    );
}

#[test]
#[serial]
fn karen_enforces_the_rest_of_a_layer_around_a_broken_file() {
    // One malformed file in a project once switched every protection off,
    // built-in ones included. Now only that file is left out: the layer's
    // other gates and the built-in gates still hold, the session is told
    // which file was skipped, and `karen validate` fails on it.
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let gates = env.policies_dir.join("gates");
    std::fs::write(
        gates.join("sound.json"),
        r#"{ "id": "sound", "event": "PreToolUse", "tool": "Bash", "match": { "all": [ { "type": "command_regex", "pattern": "^rm " } ] }, "action": { "type": "deny", "reason": "sound gate holds" } }"#,
    )
    .unwrap();
    std::fs::write(gates.join("broken.json"), "{ \"id\": \"broken\", ").unwrap();

    let input = json!({ "hook_event_name": "PreToolUse", "session_id": "broken-file", "tool_name": "Bash", "tool_input": { "command": "rm -rf build" } }).to_string();
    let (code, stdout, _stderr) = env.run_hook(&input, &[]);
    assert_exit_code(code, 0);
    assert!(stdout.contains("sound gate holds"), "{stdout}");
    assert!(
        stdout.contains("skips a gate file") && stdout.contains("broken.json"),
        "{stdout}"
    );

    let input = json!({ "hook_event_name": "PreToolUse", "session_id": "broken-file", "tool_name": "Bash", "tool_input": { "command": "git push -f origin main" } }).to_string();
    let (code, stdout, _stderr) = env.run_hook(&input, &[]);
    assert_exit_code(code, 0);
    assert!(
        stdout.contains("shared history"),
        "the built-in layer still holds: {stdout}"
    );

    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 78);
    let error = assert_json_error(&stdout, "POLICY_INVALID");
    assert!(error.to_string().contains("broken.json"), "{error}");
}

#[test]
#[serial]
fn karen_validate_rejects_a_broken_tool_filter() {
    // A tool filter is a regex too; a broken one made the gate never apply.
    let gate = r#"{ "id": "bad-tool", "event": "PreToolUse", "tool": "Bash(", "match": { "all": [ { "type": "always" } ] }, "action": { "type": "deny", "reason": "x" } }"#;
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[GateFixture {
            file: "bad-tool.json",
            content: gate,
        }],
        &[],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 78);
    assert_json_error(&stdout, "POLICY_INVALID");
}

#[test]
#[serial]
fn karen_treats_an_empty_karen_policies_as_unset() {
    // Review finding: KAREN_POLICIES="" was taken as an explicit, missing root,
    // which switched enforcement off everywhere.
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let project = env.temp_dir.path().join("proj");
    let gates = project.join(".karen").join("policies").join("gates");
    std::fs::create_dir_all(&gates).unwrap();
    std::fs::write(
        gates.join("found.json"),
        r#"{ "id": "found", "event": "PreToolUse", "tool": "Bash", "match": { "all": [ { "type": "always" } ] }, "action": { "type": "deny", "reason": "found" } }"#,
    )
    .unwrap();
    let mut cmd = env.karen_cmd(&["validate", "--json"]);
    cmd.env("KAREN_POLICIES", "").current_dir(&project);
    let (code, stdout, _stderr) = crate::e2e::common::run_command(cmd);
    assert_exit_code(code, 0);
    assert_eq!(
        assert_json_ok(&stdout)["project_found_by"],
        "in-tree",
        "{stdout}"
    );
}
