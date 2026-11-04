// tests/e2e/facts/session.rs
// The Karen who tests session identity: what the payload says about itself,
// and what she refuses to read off the allowlist.

use crate::e2e::common::{
    GateFixture, TestEnv, assert_evaluate_action, assert_exit_code, assert_json_ok,
};
use serde_json::json;
use serial_test::serial;

/// The project layer's config, with an env allowlist it has no say in.
const PROJECT_ENV_CONFIG: &str = r#"# Personal Rules engine configuration.
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

[facts]
env = ["KAREN_TEST_PROJECT_VAR"]
"#;

/// A user layer at `<home>/.karen/policies`, the only layer whose env
/// allowlist is read.
fn user_layer(env: &str, home: &std::path::Path) {
    let root = home.join(".karen").join("policies");
    std::fs::create_dir_all(root.join("gates")).expect("create user gates dir");
    std::fs::create_dir_all(root.join("rules")).expect("create user rules dir");
    std::fs::write(
        root.join("config.toml"),
        format!("# User layer.\n\n[state]\nbackend = \"memory\"\n\n[facts]\nenv = {env}\n"),
    )
    .expect("write user config");
}

/// A gate that matches only when `field` equals `value`. Test values leak:
/// a fixture lives as long as the process that loads it.
fn equals_gate(file: &'static str, id: &str, field: &str, value: &str) -> GateFixture {
    let content: &'static str = Box::leak(
        format!(
            r#"{{
  "id": "{id}",
  "description": "The test gate for {field}.",
  "event": "PreToolUse",
  "match": {{ "all": [ {{ "type": "field_equals", "field": "{field}", "value": "{value}" }} ] }},
  "action": {{ "type": "deny", "reason": "identity matched" }}
}}"#
        )
        .into_boxed_str(),
    );
    GateFixture { file, content }
}

fn pretool(session: Option<&str>, facts: &serde_json::Value) -> String {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": session.unwrap_or(""),
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "facts": facts,
    })
    .to_string()
}

fn evaluate(env: &TestEnv, input: &str, vars: &[(&str, &str)]) -> serde_json::Value {
    let (code, stdout, _stderr) = env.run_evaluate_stdin_with_env(input, &["--json"], vars);
    assert_exit_code(code, 0);
    assert_json_ok(&stdout)
}

#[test]
#[serial]
fn karen_echoes_the_sessions_own_id_as_a_fact() {
    // The payload's session id, as the runtime said it: a gate keys on
    // identity without reaching past the facts.
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[equals_gate(
            "session-id.json",
            "session-id",
            "facts.session_id",
            "identity-1",
        )],
        &[],
    );
    let data = evaluate(&env, &pretool(Some("identity-1"), &json!({})), &[]);
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_leaves_the_session_id_fact_absent_when_the_payload_carries_none() {
    // No session id in the payload: the fact is absent, and a gate matching on
    // any value stays quiet. Absence is not a value, not even "unknown".
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[equals_gate(
            "session-id-absent.json",
            "session-id-absent",
            "facts.session_id",
            "",
        )],
        &[],
    );
    let input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
    })
    .to_string();
    let data = evaluate(&env, &input, &[]);
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_reads_an_allowlisted_variable_from_her_own_environment() {
    // The user layer allowlists the name, the variable is set, and the gate
    // reads exactly that: one hand into one named pocket.
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[equals_gate(
            "env-allowed.json",
            "env-allowed",
            "facts.env.KAREN_TEST_ALLOWED",
            "prod",
        )],
        &[],
    );
    user_layer(r#"["KAREN_TEST_ALLOWED"]"#, &env.home_dir);
    let data = evaluate(
        &env,
        &pretool(Some("env-1"), &json!({})),
        &[("KAREN_TEST_ALLOWED", "prod")],
    );
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_leaves_an_allowlisted_variable_absent_when_it_is_unset() {
    // On the list, not in the environment: absent, never the empty string a
    // match on "" would otherwise catch.
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[equals_gate(
            "env-unset.json",
            "env-unset",
            "facts.env.KAREN_TEST_UNSET",
            "",
        )],
        &[],
    );
    user_layer(r#"["KAREN_TEST_UNSET"]"#, &env.home_dir);
    let data = evaluate(&env, &pretool(Some("env-2"), &json!({})), &[]);
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_never_reads_a_variable_the_project_layer_tries_to_allowlist() {
    // The security test: the variable is SET and the project layer names it,
    // but the user layer does not — so the fact stays absent and no gate can
    // trip on a value repository content tried to expose.
    let env = TestEnv::with_config(
        PROJECT_ENV_CONFIG,
        &[equals_gate(
            "env-project.json",
            "env-project",
            "facts.env.KAREN_TEST_PROJECT_VAR",
            "leak",
        )],
        &[],
    );
    let data = evaluate(
        &env,
        &pretool(Some("env-3"), &json!({})),
        &[("KAREN_TEST_PROJECT_VAR", "leak")],
    );
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_ignores_a_payloads_own_env_facts() {
    // The bridge hands her an `env` object: it never reaches a gate. The
    // variable's real value (set in her environment, allowlisted by the user
    // layer) is what matches, not the payload's word.
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[equals_gate(
            "env-payload.json",
            "env-payload",
            "facts.env.KAREN_TEST_ALLOWED",
            "real",
        )],
        &[],
    );
    user_layer(r#"["KAREN_TEST_ALLOWED"]"#, &env.home_dir);
    let data = evaluate(
        &env,
        &pretool(
            Some("env-4"),
            &json!({ "env": { "KAREN_TEST_ALLOWED": "forged" } }),
        ),
        &[("KAREN_TEST_ALLOWED", "real")],
    );
    assert_evaluate_action(&data, "deny");
    // And the forged value never matches anything:
    let forged = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[equals_gate(
            "env-forged.json",
            "env-forged",
            "facts.env.KAREN_TEST_ALLOWED",
            "forged",
        )],
        &[],
    );
    user_layer(r#"["KAREN_TEST_ALLOWED"]"#, &forged.home_dir);
    let data = evaluate(
        &forged,
        &pretool(
            Some("env-5"),
            &json!({ "env": { "KAREN_TEST_ALLOWED": "forged" } }),
        ),
        &[("KAREN_TEST_ALLOWED", "real")],
    );
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_validate_names_a_project_env_allowlist_and_off_list_gates() {
    // `karen validate` reports both: the project layer that tried to
    // allowlist a name, and the gate that names a variable no allowlist
    // carries — a gate that can never match is a silent no-op otherwise.
    let env = TestEnv::with_config(
        PROJECT_ENV_CONFIG,
        &[equals_gate(
            "env-off-list.json",
            "env-off-list",
            "facts.env.KAREN_TEST_NOT_LISTED",
            "x",
        )],
        &[],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    // Warnings do not fail validation: the rest of the policy is fine.
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let issues = data["issues"].as_array().expect("issues array");
    let text: Vec<String> = issues
        .iter()
        .map(|i| {
            format!(
                "{}: the {} layer {}",
                i["kind"].as_str().unwrap_or(""),
                i["layer"].as_str().unwrap_or(""),
                i["message"].as_str().unwrap_or("")
            )
        })
        .collect();
    assert!(
        text.iter()
            .any(|line| line.contains("sets [facts] env (KAREN_TEST_PROJECT_VAR)")),
        "validate must name the project layer's allowlist attempt: {text:?}"
    );
    assert!(
        text.iter()
            .any(|line| line.contains("`env-off-list` matches facts.env.KAREN_TEST_NOT_LISTED")),
        "validate must name the off-allowlist gate: {text:?}"
    );
}

#[test]
#[serial]
fn karen_validate_names_each_off_list_variable_once_with_its_prefix() {
    // A gate that matches a variable and exempts the same one reads it twice;
    // she names it once. A second variable gets its own full name too.
    const TWO_VARS: &str = r#"{
  "id": "env-twice",
  "description": "Reads one variable twice and another once.",
  "event": "PreToolUse",
  "match": {
    "all": [
      { "type": "field_regex", "field": "facts.env.KAREN_TEST_FIRST", "pattern": "." },
      { "type": "field_equals", "field": "facts.env.KAREN_TEST_SECOND", "value": "x" }
    ],
    "none": [ { "type": "field_equals", "field": "facts.env.KAREN_TEST_FIRST", "value": "y" } ]
  },
  "action": { "type": "deny", "reason": "identity matched" }
}"#;
    let env = TestEnv::with_config(
        crate::e2e::common::DEFAULT_CONFIG,
        &[GateFixture {
            file: "env-twice.json",
            content: TWO_VARS,
        }],
        &[],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let messages: Vec<&str> = data["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .filter_map(|i| i["message"].as_str())
        .collect();
    assert!(
        messages.iter().any(|m| m.contains(
            "`env-twice` matches facts.env.KAREN_TEST_FIRST, facts.env.KAREN_TEST_SECOND not on"
        )),
        "each variable once, each with its prefix: {messages:?}"
    );
}

#[test]
#[serial]
fn karen_validate_stays_quiet_about_a_disabled_gates_env_reads() {
    // A disabled gate reads nothing, so a warning about what it would read is
    // noise: the starter pack ships such gates and must stay silent until the
    // user opts in. The same gate enabled is named, so the quiet is not vacuous.
    fn gate(enabled: bool) -> &'static str {
        Box::leak(
            format!(
                r#"{{
  "id": "env-sleeper",
  "description": "Reads and templates an env fact it may not have.",
  "enabled": {enabled},
  "event": "PreToolUse",
  "match": {{ "all": [ {{ "type": "field_regex", "field": "facts.env.KAREN_TEST_ASLEEP", "pattern": "." }} ] }},
  "action": {{ "type": "deny", "reason": "lane {{facts.env.KAREN_TEST_ASLEEP}} stops" }}
}}"#
            )
            .into_boxed_str(),
        )
    }
    let messages_for = |enabled: bool| -> Vec<String> {
        let env = TestEnv::with_config(
            crate::e2e::common::DEFAULT_CONFIG,
            &[GateFixture {
                file: "env-sleeper.json",
                content: gate(enabled),
            }],
            &[],
        );
        let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
        assert_exit_code(code, 0);
        let data = assert_json_ok(&stdout);
        data["issues"]
            .as_array()
            .expect("issues array")
            .iter()
            .filter_map(|i| i["message"].as_str().map(str::to_string))
            .collect()
    };
    let awake = messages_for(true);
    assert!(
        awake.iter().any(|m| m.contains("`env-sleeper` matches")),
        "an enabled gate is named: {awake:?}"
    );
    assert!(
        awake.iter().any(|m| m.contains("`env-sleeper` templates")),
        "an enabled gate's templated env is named: {awake:?}"
    );
    let asleep = messages_for(false);
    assert!(
        asleep.iter().all(|m| !m.contains("env-sleeper")),
        "a disabled gate is not named: {asleep:?}"
    );
}
