// tests/e2e/policy/matchers.rs
// The Karen who tests the numeric matchers, because a gate that compares
// numbers she never read is just an opinion in a JSON file.

use crate::e2e::common::{
    DEFAULT_CONFIG, GateFixture, TestEnv, assert_evaluate_action, assert_exit_code, assert_json_ok,
};
use serde_json::json;
use serial_test::serial;

/// A gate that matches only when the numeric `kind` condition holds on `field`.
/// Test values leak: a fixture lives as long as the process that loads it.
fn number_gate(file: &'static str, id: &str, kind: &str, field: &str, value: f64) -> GateFixture {
    let content: &'static str = Box::leak(
        format!(
            r#"{{
  "id": "{id}",
  "description": "The test gate for a numeric matcher.",
  "event": "PreToolUse",
  "match": {{ "all": [ {{ "type": "{kind}", "field": "{field}", "value": {value} }} ] }},
  "action": {{ "type": "deny", "reason": "number matched" }}
}}"#
        )
        .into_boxed_str(),
    );
    GateFixture { file, content }
}

/// A `PreToolUse` payload carrying the facts she reads.
fn pretool(facts: &serde_json::Value) -> String {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": "matcher-numbers",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "facts": facts,
    })
    .to_string()
}

fn evaluate(env: &TestEnv, input: &str) -> serde_json::Value {
    let (code, stdout, _stderr) = env.run_evaluate_stdin(input, &["--json"]);
    assert_exit_code(code, 0);
    assert_json_ok(&stdout)
}

#[test]
#[serial]
fn field_gte_matches_at_and_above_the_value() {
    // Two fifths is the line, and standing on it counts as over it.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "gte.json",
            "gte",
            "field_gte",
            "facts.crankiness",
            0.4,
        )],
        &[],
    );
    let at = evaluate(&env, &pretool(&json!({ "crankiness": 0.4 })));
    assert_evaluate_action(&at, "deny");
    let above = evaluate(&env, &pretool(&json!({ "crankiness": 0.45 })));
    assert_evaluate_action(&above, "deny");
}

#[test]
#[serial]
fn field_gte_stays_quiet_below_the_value() {
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "gte-below.json",
            "gte-below",
            "field_gte",
            "facts.crankiness",
            0.4,
        )],
        &[],
    );
    let below = evaluate(&env, &pretool(&json!({ "crankiness": 0.39 })));
    assert_evaluate_action(&below, "allow");
}

#[test]
#[serial]
fn field_gte_never_matches_an_absent_field() {
    // Absence is not a number, and she does not round it down to zero: a
    // missing field never clears a bar of zero or any other height.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "gte-absent.json",
            "gte-absent",
            "field_gte",
            "facts.crankiness",
            0.0,
        )],
        &[],
    );
    let absent = evaluate(&env, &pretool(&json!({})));
    assert_evaluate_action(&absent, "allow");
}

#[test]
#[serial]
fn field_gte_never_matches_a_non_numeric_field() {
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "gte-text.json",
            "gte-text",
            "field_gte",
            "facts.crankiness",
            0.4,
        )],
        &[],
    );
    let text = evaluate(&env, &pretool(&json!({ "crankiness": "high" })));
    assert_evaluate_action(&text, "allow");
    let null = evaluate(&env, &pretool(&json!({ "crankiness": null })));
    assert_evaluate_action(&null, "allow");
    let list = evaluate(&env, &pretool(&json!({ "crankiness": [0.9] })));
    assert_evaluate_action(&list, "allow");
}

#[test]
#[serial]
fn field_lte_matches_at_and_below_the_value() {
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "lte.json",
            "lte",
            "field_lte",
            "facts.crankiness",
            0.4,
        )],
        &[],
    );
    let at = evaluate(&env, &pretool(&json!({ "crankiness": 0.4 })));
    assert_evaluate_action(&at, "deny");
    let below = evaluate(&env, &pretool(&json!({ "crankiness": 0.25 })));
    assert_evaluate_action(&below, "deny");
}

#[test]
#[serial]
fn field_lte_stays_quiet_above_the_value() {
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "lte-above.json",
            "lte-above",
            "field_lte",
            "facts.crankiness",
            0.4,
        )],
        &[],
    );
    let above = evaluate(&env, &pretool(&json!({ "crankiness": 0.41 })));
    assert_evaluate_action(&above, "allow");
}

#[test]
#[serial]
fn numeric_matchers_read_whole_numbers_from_json() {
    // JSON writes `4336`, the gate reads `4336` as a number, and no decimal
    // point is needed for the two to agree.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "whole-gte.json",
            "whole-gte",
            "field_gte",
            "facts.tokens",
            4336.0,
        )],
        &[],
    );
    let data = evaluate(&env, &pretool(&json!({ "tokens": 4336 })));
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn field_gte_on_the_ratio_reads_the_two_facts_it_divides() {
    // A ratio gate names what it means: she collects the two facts it divides,
    // and 500 tokens in a 1000-token window says one half out loud.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "ratio-gte.json",
            "ratio-gte",
            "field_gte",
            "facts.context_ratio",
            0.4,
        )],
        &[],
    );
    let data = evaluate(
        &env,
        &pretool(&json!({ "context_tokens": 500, "context_window": 1000 })),
    );
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn field_gte_on_the_ratio_stays_quiet_when_the_ratio_is_absent() {
    // Tokens without a window is not a ratio, and she does not invent one.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[number_gate(
            "ratio-absent.json",
            "ratio-absent",
            "field_gte",
            "facts.context_ratio",
            0.0,
        )],
        &[],
    );
    let data = evaluate(&env, &pretool(&json!({ "context_tokens": 500 })));
    assert_evaluate_action(&data, "allow");
}
