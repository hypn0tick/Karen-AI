// tests/e2e/policy/gates.rs
// The Karen who tests gates because she's personally seen every one of these patterns in the wild.

use crate::e2e::common::{
    TestEnv, assert_evaluate_action, assert_exit_code, assert_hook_deny_once_first,
    assert_hook_warned_retry, assert_json_ok,
};
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn karen_verifies_gate_commit_attribution_denies_robot_emoji() {
    // The Karen who blocks attribution trailers because she doesn't give credit to her shovel.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-commit-1",
        "tool_name": "Bash",
        "tool_input": { "command": "git commit -m \"x 🤖\"" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "deny");
    assert!(data["reason"].as_str().unwrap().contains("attribution"));
}

#[test]
#[serial]
fn karen_verifies_gate_commit_attribution_allows_clean() {
    // The Karen who allows clean commits without attribution.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-commit-2",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_verifies_gate_commit_attribution_denies_co_authored_by() {
    // The Karen who blocks Co-Authored-By trailers.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-commit-3",
        "tool_name": "Bash",
        "tool_input": { "command": "git commit -m \"x\n\nCo-Authored-By: Claude <noreply@anthropic.com>\"" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_verifies_gate_jq_yaml_first_strike_denies() {
    // The Karen who redirects jq to yq because jq literally cannot parse YAML/TOML.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-jq-1",
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "deny");
    assert!(data["reason"].as_str().unwrap().contains("yq"));
}

#[test]
#[serial]
fn karen_verifies_gate_jq_yaml_second_strike_allows() {
    // The Karen who allows jq on retry (deliberate fallback) — through the
    // hook, because strike progression is the hook's job. Evaluate is a
    // rehearsal; the gate's memory lives on the enforcement path.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-jq-2",
        "tool_name": "Bash",
        "tool_input": { "command": "jq . file.yaml" }
    });
    let (code1, stdout1, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code1, 0);
    assert_hook_deny_once_first(&stdout1, Some("yq"));
    let (code2, stdout2, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code2, 0);
    // The Karen who allows the retry — with the gate's closing words and a
    // nudge so the agent remembers which shortcut was tolerated.
    assert_hook_warned_retry(&stdout2, Some("jq on a YAML/TOML"));
}

#[test]
#[serial]
fn karen_verifies_gate_web_search_first_strike_denies() {
    // The Karen who redirects web search to Brave/Exa because native search is lazy.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-web-1",
        "tool_name": "WebSearch",
        "tool_input": { "query": "rust" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "deny");
    assert!(data["reason"].as_str().unwrap().contains("Brave"));
}

#[test]
#[serial]
fn karen_verifies_gate_web_search_second_strike_allows() {
    // The Karen who allows native web search on retry — through the hook,
    // where the strike is actually recorded.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-web-2",
        "tool_name": "WebSearch",
        "tool_input": { "query": "rust" }
    });
    let (code1, stdout1, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code1, 0);
    assert_hook_deny_once_first(&stdout1, Some("Brave"));
    let (code2, stdout2, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code2, 0);
    assert_hook_warned_retry(&stdout2, Some("web search"));
}

#[test]
#[serial]
fn karen_verifies_gate_head_modify_file() {
    // The Karen who ports head to Select-Object because she's seen CI pipelines fail on Windows.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-head-1",
        "tool_name": "Bash",
        "tool_input": { "command": "head -5 file.txt" }
    });
    let (code, stdout, _stderr) =
        env.run_evaluate_stdin(&input.to_string(), &["--json", "--include-content"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "modify");
    let modified = &data["modified_input"];
    assert!(
        modified["command"]
            .as_str()
            .unwrap()
            .contains("Get-Content")
    );
    assert!(
        modified["command"]
            .as_str()
            .unwrap()
            .contains("Select-Object -First 5")
    );
}

#[test]
#[serial]
fn karen_verifies_gate_head_modify_pipeline() {
    // The Karen who converts pipeline head to Select-Object.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-head-2",
        "tool_name": "Bash",
        "tool_input": { "command": "cat file.txt | head -n 10" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "modify");
    let modified = &data["modified_input"];
    assert!(
        modified["command"]
            .as_str()
            .unwrap()
            .contains("Select-Object -First 10")
    );
}

#[test]
#[serial]
fn karen_verifies_gate_cargo_quality_first_strike_denies() {
    // The Karen who practices what she preaches demands clean code.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-cargo-1",
        "tool_name": "Bash",
        "tool_input": { "command": "git push" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "deny");
    assert!(data["reason"].as_str().unwrap().contains("cargo fmt"));
}

#[test]
#[serial]
fn karen_verifies_gate_cargo_quality_second_strike_allows() {
    // The Karen who allows on retry but expects fixes next time — recorded
    // where it counts, on the hook path.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-cargo-2",
        "tool_name": "Bash",
        "tool_input": { "command": "git push" }
    });
    let (code1, stdout1, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code1, 0);
    assert_hook_deny_once_first(&stdout1, Some("cargo fmt"));
    let (code2, stdout2, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code2, 0);
    assert_hook_warned_retry(&stdout2, Some("fix the warnings"));
}

#[test]
#[serial]
fn karen_verifies_gate_backticks_first_strike_denies() {
    // The Karen who insists on formatting discipline.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-backticks-1",
        "tool_name": "Write",
        "tool_input": { "file_path": "test.rs", "content": "fn main() { cargo build(); }" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "deny");
    assert!(data["reason"].as_str().unwrap().contains("backticks"));
}

#[test]
#[serial]
fn karen_verifies_gate_backticks_second_strike_allows() {
    // The Karen who allows on retry but expects formatting fixes — recorded
    // on the hook path, where the warning actually lands.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-backticks-2",
        "tool_name": "Write",
        "tool_input": { "file_path": "test.rs", "content": "fn main() { cargo build(); }" }
    });
    let (code1, stdout1, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code1, 0);
    assert_hook_deny_once_first(&stdout1, Some("backticks"));
    let (code2, stdout2, _) = env.run_hook(&input.to_string(), &[]);
    assert_exit_code(code2, 0);
    assert_hook_warned_retry(&stdout2, Some("formatting"));
}

#[test]
#[serial]
fn karen_verifies_gate_jsonc_test_never_fires() {
    // The Karen who verifies jsonc-test-gate exists but shouldn't fire in normal use.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "gate-jsonc",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" }
    });
    let (code, stdout, _stderr) = env.run_evaluate_stdin(&input.to_string(), &["--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert_evaluate_action(&data, "allow");
    // jsonc-test-gate should not fire
    let fired = data["fired_policies"].as_array().unwrap();
    assert!(
        !fired
            .iter()
            .any(|v| v.as_str().unwrap() == "jsonc-test-gate")
    );
}
