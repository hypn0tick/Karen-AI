// tests/e2e/cli/schema.rs
// The Karen who schemas schema because self-documentation is the only documentation that stays current.

use crate::e2e::common::{
    TestEnv, assert_exit_code, assert_json_error, assert_json_ok, assert_schema_commands,
};
use serial_test::serial;

#[test]
#[serial]
fn karen_schema_lists_all_commands() {
    // The Karen who verifies schema lists every command.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["schema", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_schema_commands(
        &data,
        &[
            "hook",
            "validate",
            "check",
            "evaluate",
            "schema",
            "setup",
            "completion",
        ],
    );
}

#[test]
#[serial]
fn karen_schema_includes_types() {
    // The Karen who verifies schema includes all type vocabularies.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["schema", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let types = &data["types"];
    assert!(types["events"].is_array());
    assert!(types["adapters"].is_array());
    assert!(types["conditions"].is_array());
    assert!(types["actions"].is_array());
    assert!(types["exit_codes"].is_object());
    // Check specific values
    assert!(types["events"].as_array().unwrap().len() >= 11);
    assert!(
        types["adapters"]
            .as_array()
            .unwrap()
            .contains(&"claude".into())
    );
    assert!(
        types["adapters"]
            .as_array()
            .unwrap()
            .contains(&"claude-classic".into())
    );
}

#[test]
#[serial]
fn karen_schema_includes_invariants() {
    // The Karen who verifies schema includes invariants for agent guidance.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["schema", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let invariants = data["invariants"].as_array().unwrap();
    assert!(invariants.len() >= 4);
    let invariant_text: Vec<&str> = invariants.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(
        invariant_text
            .iter()
            .any(|s| s.contains("stdout carries data only"))
    );
    assert!(
        invariant_text
            .iter()
            .any(|s| s.contains("hook output is the runtime protocol"))
    );
    assert!(
        invariant_text
            .iter()
            .any(|s| s.contains("evaluate is read-only"))
    );
}

#[test]
#[serial]
fn karen_schema_topic_filters_to_one_command() {
    // The Karen who verifies schema topic filters to one command.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["schema", "hook", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let commands = data["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["name"], "hook");
}

#[test]
#[serial]
fn karen_schema_unknown_topic_errors() {
    // The Karen who rejects unknown schema topics.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["schema", "nonexistent", "--json"]);
    assert_exit_code(code, 2);
    assert_json_error(&stdout, "UNKNOWN_SCHEMA_TOPIC");
}

#[test]
#[serial]
fn karen_schema_human_output_is_pretty_json() {
    // The Karen who verifies human schema output is pretty JSON.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["schema"]);
    assert_exit_code(code, 0);
    // Should be valid JSON
    let _: serde_json::Value =
        serde_json::from_str(&stdout).expect("human schema output must be valid JSON");
    // Should be pretty (multi-line)
    assert!(stdout.contains('\n'));
}
