// tests/e2e/policy/layers.rs
// Built-in, user and project layers, merged by gate id.

use crate::e2e::common::fixtures::GateFixture;
use crate::e2e::common::{
    TestEnv, assert_exit_code, assert_json_error, assert_json_ok, run_command,
};
use serde_json::{Value, json};
use serial_test::serial;

/// Leaked: `GateFixture` holds `&'static str`, and a test process is short.
fn deny_gate(id: &str, needle: &str, reason: &str, extra: &str) -> &'static str {
    let gate = format!(
        r#"{{ "id": "{id}", "event": "PreToolUse", "tool": "Bash", {extra} "match": {{ "all": [ {{ "type": "command_regex", "pattern": "{needle}" }} ] }}, "action": {{ "type": "deny", "reason": "{reason}" }} }}"#
    );
    Box::leak(gate.into_boxed_str())
}

fn write_user(env: &TestEnv, file: &str, content: &str) {
    let dir = env.home_dir.join(".karen").join("policies").join("gates");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), content).unwrap();
}

fn evaluate(env: &TestEnv, command: &str) -> Value {
    let input = json!({ "hook_event_name": "PreToolUse", "session_id": "layers", "tool_name": "Bash", "tool_input": { "command": command } }).to_string();
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input, &["--json"]);
    assert_exit_code(code, 0);
    assert_json_ok(&stdout)
}

fn verdict(data: &Value) -> (String, String, Vec<String>) {
    let action = data["action"].as_str().unwrap_or("").to_string();
    let reason = data["reason"].as_str().unwrap_or("").to_string();
    let layers = data["fired_layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{}:{}",
                f["id"].as_str().unwrap(),
                f["layer"].as_str().unwrap()
            )
        })
        .collect();
    (action, reason, layers)
}

#[test]
#[serial]
fn karen_merges_user_and_project_layers() {
    let gate1 = deny_gate("project-gate", "project-only", "project says no", "");
    let env = TestEnv::with_config(
        "",
        &[GateFixture {
            file: "project.json",
            content: gate1,
        }],
        &[],
    );
    write_user(
        &env,
        "user.json",
        deny_gate("user-gate", "user-only", "user says no", ""),
    );

    let (action, _, fired) = verdict(&evaluate(&env, "echo user-only"));
    assert_eq!(action, "deny");
    assert_eq!(fired, ["user-gate:user"]);
    let (action, _, fired) = verdict(&evaluate(&env, "echo project-only"));
    assert_eq!(action, "deny");
    assert_eq!(fired, ["project-gate:project"]);
}

#[test]
#[serial]
fn karen_replaces_a_gate_only_on_explicit_override() {
    let gate2 = deny_gate(
        "shared",
        "shared",
        "project wording",
        r#""override": true,"#,
    );
    let replaced = TestEnv::with_config(
        "",
        &[GateFixture {
            file: "shared.json",
            content: gate2,
        }],
        &[],
    );
    write_user(
        &replaced,
        "shared.json",
        deny_gate("shared", "shared", "user wording", ""),
    );
    let (action, reason, fired) = verdict(&evaluate(&replaced, "echo shared"));
    assert_eq!(action, "deny");
    assert!(reason.contains("project wording"), "{reason}");
    assert_eq!(fired, ["shared:project"]);

    // A reused id without override is refused: the first definition stays,
    // and validate names the refusal.
    let gate3 = deny_gate("shared", "shared", "project wording", "");
    let reused = TestEnv::with_config(
        "",
        &[GateFixture {
            file: "shared.json",
            content: gate3,
        }],
        &[],
    );
    write_user(
        &reused,
        "shared.json",
        deny_gate("shared", "shared", "user wording", ""),
    );
    let (_, reason, fired) = verdict(&evaluate(&reused, "echo shared"));
    assert!(reason.contains("user wording"), "{reason}");
    assert_eq!(fired, ["shared:user"]);
    let (code, stdout, _) = reused.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 78);
    assert_json_error(&stdout, "POLICY_INVALID");
}

#[test]
#[serial]
fn karen_keeps_a_locked_gate_enforced() {
    let gate4 = deny_gate(
        "guard",
        "guarded",
        "project took over",
        r#""override": true,"#,
    );
    let env = TestEnv::with_config(
        "[policy]\ndisable = [\"guard\"]\n",
        &[GateFixture {
            file: "guard.json",
            content: gate4,
        }],
        &[],
    );
    write_user(
        &env,
        "guard.json",
        deny_gate("guard", "guarded", "the user's guard", r#""locked": true,"#),
    );

    let (action, reason, fired) = verdict(&evaluate(&env, "echo guarded"));
    assert_eq!(action, "deny");
    assert!(reason.contains("the user's guard"), "{reason}");
    assert_eq!(fired, ["guard:user"]);

    let (code, stdout, _) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 78);
    assert_json_error(&stdout, "POLICY_INVALID");

    // The hook still enforces, and tells the session once.
    let input = json!({ "hook_event_name": "PreToolUse", "session_id": "locked", "tool_name": "Bash", "tool_input": { "command": "echo guarded" } }).to_string();
    let (_, first, _) = env.run_hook(&input, &[]);
    assert!(first.contains("Karen-AI policy merge"), "{first}");
    assert!(first.contains("locked"), "{first}");
    let (_, second, _) = env.run_hook(&input, &[]);
    assert!(
        !second.contains("Karen-AI policy merge"),
        "told once per session: {second}"
    );
}

#[test]
#[serial]
fn karen_disables_gates_by_id() {
    let env = TestEnv::with_config(
        "[policy]\ndisable = [\"nag\", \"karen-no-force-push-default-branch\"]\n",
        &[],
        &[],
    );
    write_user(&env, "nag.json", deny_gate("nag", "nagged", "nag", ""));
    let (action, _, _) = verdict(&evaluate(&env, "echo nagged"));
    assert_eq!(action, "allow");
    let (action, _, _) = verdict(&evaluate(&env, "git push --force origin main"));
    assert_eq!(action, "allow", "a built-in gate can be disabled");

    let builtin = TestEnv::with_config("", &[], &[]);
    let (action, _, fired) = verdict(&evaluate(&builtin, "git push --force origin main"));
    assert_eq!(action, "deny");
    assert_eq!(fired, ["karen-no-force-push-default-branch:builtin"]);
}

#[test]
#[serial]
fn karen_finds_the_project_layer_above_the_working_directory() {
    let env = TestEnv::with_config("", &[], &[]);
    let project = env.temp_dir.path().join("proj");
    let gates = project.join(".karen").join("policies").join("gates");
    std::fs::create_dir_all(&gates).unwrap();
    std::fs::write(
        gates.join("found.json"),
        deny_gate("found", "x", "found", ""),
    )
    .unwrap();
    let work = project.join("src").join("deep");
    std::fs::create_dir_all(&work).unwrap();

    let layers_seen = |cwd: &std::path::Path| {
        let mut cmd = env.karen_cmd(&["validate", "--json"]);
        cmd.env_remove("KAREN_POLICIES").current_dir(cwd);
        let (code, stdout, _) = run_command(cmd);
        assert_exit_code(code, 0);
        assert_json_ok(&stdout)["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["layer"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };

    assert_eq!(layers_seen(&work), ["project"]);

    // Under the home directory, the user layer is not found twice.
    write_user(&env, "u.json", deny_gate("u", "y", "u", ""));
    let under_home = env.home_dir.join("work");
    std::fs::create_dir_all(&under_home).unwrap();
    assert_eq!(layers_seen(&under_home), ["user"]);
}
