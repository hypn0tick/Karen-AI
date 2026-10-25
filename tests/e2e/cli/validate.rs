// tests/e2e/cli/validate.rs
// The Karen who validates CLI contracts because she doesn't trust untested flags.

use crate::e2e::common::{
    TestEnv, assert_exit_code, assert_fields_filtered, assert_json_error, assert_json_ok,
    assert_policy_health,
};
use serial_test::serial;

#[test]
#[serial]
fn karen_validates_json_envelope() {
    // The Karen who knows validate --json must return a proper envelope.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_policy_health(&data, 7, 7);
}

#[test]
#[serial]
fn karen_validates_human_output() {
    // The Karen who checks human-readable output isn't broken.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["validate"]);
    assert_exit_code(code, 0);
    assert!(stdout.contains("policies:"));
    assert!(stdout.contains("gates:"));
    assert!(stdout.contains("rules:"));
    assert!(stdout.contains("Next steps:"));
}

#[test]
#[serial]
fn karen_validates_quiet_suppresses_output() {
    // The Karen who verifies --quiet actually quiets.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--quiet"]);
    assert_exit_code(code, 0);
    assert!(stdout.trim().is_empty(), "quiet should produce no stdout");
}

#[test]
#[serial]
fn karen_check_lists_all_gates_and_rules() {
    // The Karen who checks that check lists every gate and rule.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["check", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_policy_health(&data, 7, 7);
    let gates = data["gates"].as_array().unwrap();
    assert!(gates.len() >= 7, "expected at least 7 gates");
    for gate in gates {
        assert!(gate["id"].is_string(), "gate missing id");
        assert!(gate["event"].is_string(), "gate missing event");
        assert!(gate["priority"].is_number(), "gate missing priority");
        assert!(gate["action"].is_string(), "gate missing action");
        assert!(gate["enabled"].is_boolean(), "gate missing enabled");
        assert!(gate["tags"].is_array(), "gate missing tags");
    }
}

#[test]
#[serial]
fn karen_validate_fields_filter_gate_count_rule_count() {
    // The Karen who filters fields because tokens aren't free.
    let env = TestEnv::new();
    let (code, stdout, _stderr) =
        env.run_karen(&["validate", "--json", "--fields", "gate_count,rule_count"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_fields_filtered(&data, &["gate_count", "rule_count"]);
}

#[test]
#[serial]
fn karen_validate_fields_unknown_errors() {
    // The Karen who rejects unknown fields because she's strict.
    let env = TestEnv::new();
    let (code, stdout, _stderr) =
        env.run_karen(&["validate", "--json", "--fields", "nonexistent_field"]);
    assert_exit_code(code, 2);
    let error = assert_json_error(&stdout, "INVALID_FIELDS");
    assert!(error["message"].as_str().unwrap().contains("unknown field"));
}

#[test]
#[serial]
fn karen_validate_output_json_vs_output_text() {
    // The Karen who tests both --json and --output json produce same result.
    let env = TestEnv::new();
    let (code1, stdout1, _) = env.run_karen(&["validate", "--json"]);
    let (code2, stdout2, _) = env.run_karen(&["validate", "--output", "json"]);
    assert_exit_code(code1, 0);
    assert_exit_code(code2, 0);
    let data1 = assert_json_ok(&stdout1);
    let data2 = assert_json_ok(&stdout2);
    assert_eq!(data1, data2, "--json and --output json must match");
}
