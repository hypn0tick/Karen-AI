// tests/e2e/cli/check.rs
// The Karen who checks check because check should check what validate validates.

use crate::e2e::common::{TestEnv, assert_exit_code, assert_json_ok, assert_policy_health};
use serial_test::serial;

#[test]
#[serial]
fn karen_check_json_lists_gates() {
    // The Karen who verifies check --json lists all gates.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["check", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_policy_health(&data, 7, 7);
    let gates = data["gates"].as_array().unwrap();
    assert!(gates.len() >= 7);
}

#[test]
#[serial]
fn karen_check_json_lists_rules() {
    // The Karen who verifies check --json lists all rules.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["check", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let rules = data["rules"].as_array().unwrap();
    assert!(rules.len() >= 7);
    for rule in rules {
        assert!(rule["id"].is_string(), "rule missing id");
        assert!(rule["path"].is_string(), "rule missing path");
        assert!(rule["chars"].is_number(), "rule missing chars");
    }
}

#[test]
#[serial]
fn karen_check_human_output_verbose() {
    // The Karen who checks human output includes gate and rule details.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["check"]);
    assert_exit_code(code, 0);
    assert!(stdout.contains("commit-attribution"));
    assert!(stdout.contains("jq-yaml-redirect"));
    assert!(stdout.contains("web-search-redirect"));
    assert!(stdout.contains("head-to-select-object"));
    assert!(stdout.contains("cargo-quality-gate"));
    assert!(stdout.contains("backticks-in-comments"));
    assert!(stdout.contains("jsonc-test-gate"));
}
