// tests/e2e/common/assertions.rs
// The Karen who asserts because "it looks right" is not a test.

use serde_json::Value;

/// Assert that a hook protocol response is an allow (empty object)
pub fn assert_hook_allow(output: &str) {
    let value: Value = serde_json::from_str(output).expect("valid JSON output");
    assert_eq!(
        value,
        serde_json::json!({}),
        "expected allow (empty object), got: {output}"
    );
}

/// Assert that a hook protocol response is a deny with permissionDecision and reason
pub fn assert_hook_deny(output: &str, expected_reason_contains: Option<&str>) {
    let value: Value = serde_json::from_str(output).expect("valid JSON output");
    assert!(
        value.get("hookSpecificOutput").is_some(),
        "deny must have hookSpecificOutput"
    );
    assert_eq!(
        value["hookSpecificOutput"]["permissionDecision"], "deny",
        "permissionDecision must be 'deny'"
    );
    assert!(
        value["hookSpecificOutput"]["permissionDecisionReason"].is_string(),
        "permissionDecisionReason must be a string"
    );
    if let Some(needle) = expected_reason_contains {
        let reason = value["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        assert!(
            reason.contains(needle),
            "reason '{reason}' does not contain '{needle}'"
        );
    }
}

/// Assert that a hook protocol response is a first strike of `deny_once_then_warn`:
/// a real deny (permissionDecision "deny") plus the coaching in systemMessage.
pub fn assert_hook_deny_once_first(output: &str, expected_reason_contains: Option<&str>) {
    let value: Value = serde_json::from_str(output).expect("valid JSON output");
    assert!(
        value.get("systemMessage").is_some(),
        "first strike must have systemMessage"
    );
    let msg = value["systemMessage"].as_str().unwrap();
    if let Some(needle) = expected_reason_contains {
        assert!(
            msg.contains(needle),
            "systemMessage '{msg}' does not contain '{needle}'"
        );
    }
    // The Karen who denies AND explains: a first strike that only lectures
    // without blocking is a suggestion wearing a badge.
    assert!(
        value["hookSpecificOutput"]["permissionDecision"] == "deny",
        "first strike must carry permissionDecision 'deny': {output}"
    );
}

/// Assert that a hook protocol response is a warned retry (second strike of
/// `deny_once_then_warn)`: allowed, with the gate's closing words and a nudge
/// in systemMessage, and no deny decision. The Karen who lets it slide — loudly.
pub fn assert_hook_warned_retry(output: &str, expected_contains: Option<&str>) {
    let value: Value = serde_json::from_str(output).expect("valid JSON output");
    let decision = value["hookSpecificOutput"]["permissionDecision"].as_str();
    assert!(
        decision != Some("deny"),
        "a warned retry must not deny: {output}"
    );
    let msg = value["systemMessage"]
        .as_str()
        .unwrap_or_else(|| panic!("warned retry must carry a systemMessage nudge: {output}"));
    assert!(
        !msg.is_empty(),
        "warned retry systemMessage must not be empty"
    );
    if let Some(needle) = expected_contains {
        assert!(
            msg.contains(needle),
            "systemMessage '{msg}' does not contain '{needle}'"
        );
    }
}

/// Assert that a hook protocol response is a modify: `permissionDecision: "ask"`
/// with the rewritten `modifiedInput` at the top level, where the runtime reads it.
pub fn assert_hook_modify(output: &str, expected_command_contains: Option<&str>) {
    let value: Value = serde_json::from_str(output).expect("valid JSON output");
    assert!(
        value.get("hookSpecificOutput").is_some(),
        "modify must have hookSpecificOutput"
    );
    assert_eq!(
        value["hookSpecificOutput"]["permissionDecision"], "ask",
        "modify surfaces as permissionDecision 'ask'"
    );
    assert!(
        value["modifiedInput"].is_object(),
        "modifiedInput must be an object at the top level"
    );
    if let Some(needle) = expected_command_contains {
        let cmd = value["modifiedInput"]["command"].as_str().unwrap();
        assert!(
            cmd.contains(needle),
            "modified command '{cmd}' does not contain '{needle}'"
        );
    }
}

/// Assert that a hook protocol response is a `SessionStart` part with additionalContext
pub fn assert_hook_sessionstart_part(output: &str, part: usize) {
    let value: Value = serde_json::from_str(output).expect("valid JSON output");
    assert!(
        value.get("hookSpecificOutput").is_some(),
        "part {part} must have hookSpecificOutput"
    );
    assert_eq!(value["hookSpecificOutput"]["hookEventName"], "SessionStart");
    assert!(
        value["hookSpecificOutput"]["additionalContext"].is_string(),
        "part {part} missing additionalContext string"
    );
    // The Karen who verifies that rule injections are non-empty strings.
    let ctx = value["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(
        !ctx.is_empty(),
        "additionalContext must not be empty for part {part}"
    );
}

/// Assert JSON envelope has status "ok" and data field
pub fn assert_json_ok(output: &str) -> Value {
    let value: Value = serde_json::from_str(output).expect("valid JSON output");
    assert_eq!(value["status"], "ok", "status must be 'ok'");
    assert!(
        value["data"].is_object() || value["data"].is_array(),
        "data must be object or array"
    );
    value["data"].clone()
}

/// Assert JSON envelope has status "error" with expected code
pub fn assert_json_error(output: &str, expected_code: &str) -> Value {
    let value: Value = serde_json::from_str(output).expect("valid JSON output");
    assert_eq!(value["status"], "error", "status must be 'error'");
    assert_eq!(
        value["error"]["code"].as_str().unwrap(),
        expected_code,
        "error code mismatch"
    );
    value["error"].clone()
}

/// Assert validate/check output has expected gate and rule counts
pub fn assert_policy_health(data: &Value, min_gates: usize, min_rules: usize) {
    assert!(
        data["gate_count"].as_u64().unwrap() >= min_gates as u64,
        "gate_count too low"
    );
    assert!(
        data["rule_count"].as_u64().unwrap() >= min_rules as u64,
        "rule_count too low"
    );
    assert!(data["gates"].is_array(), "gates must be array");
    assert!(data["rules"].is_array(), "rules must be array");
}

/// Assert evaluate output has expected action
pub fn assert_evaluate_action(data: &Value, expected_action: &str) {
    assert_eq!(
        data["action"].as_str().unwrap(),
        expected_action,
        "action mismatch"
    );
    assert_eq!(data["dry_run"], true, "dry_run must be true");
}

/// Assert schema output has all expected commands
pub fn assert_schema_commands(data: &Value, expected: &[&str]) {
    let commands: Vec<&str> = data["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    for exp in expected {
        assert!(commands.contains(exp), "missing command '{exp}' in schema");
    }
}

/// Assert field filtering works (only requested fields present)
pub fn assert_fields_filtered(data: &Value, expected_fields: &[&str]) {
    let obj = data.as_object().unwrap();
    for field in expected_fields {
        assert!(obj.contains_key(*field), "expected field '{field}' missing");
    }
    for key in obj.keys() {
        assert!(
            expected_fields.contains(&key.as_str()),
            "unexpected field '{key}' present"
        );
    }
}

/// Assert that output is valid JSON (doesn't panic)
pub fn assert_valid_json(output: &str) -> Value {
    serde_json::from_str(output).expect("output must be valid JSON")
}

/// Assert exit code matches expected
pub fn assert_exit_code(code: i32, expected: i32) {
    assert_eq!(code, expected, "exit code {code} != expected {expected}");
}
