// tests/e2e/host_lean.rs
// The Karen who admits what this build cannot do, instead of pretending. Compiled only into the
// lean build (`--no-default-features`), where `karen run` has no terminal host behind it.

use crate::e2e::common::karen_binary_path;

#[test]
fn karen_run_in_the_lean_build_says_there_is_no_terminal_host() {
    let output = karen::proc::command(karen_binary_path())
        .args(["run", "anything"])
        .output()
        .expect("karen runs");

    assert_eq!(output.status.code(), Some(70));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no terminal host"), "{stderr}");
}

#[test]
fn karen_validate_in_the_lean_build_names_a_gate_that_reads_the_screen() {
    use crate::e2e::common::{DEFAULT_CONFIG, GateFixture, TestEnv, assert_exit_code};

    const GATE: &str = r#"{
  "id": "reads-the-screen",
  "description": "Reads a screen the lean build cannot see.",
  "event": "PreToolUse",
  "match": { "all": [ { "type": "field_equals", "field": "facts.screen.usage_limit", "value": true } ] },
  "action": { "type": "deny", "reason": "limit" }
}"#;
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[GateFixture {
            file: "reads-the-screen.json",
            content: GATE,
        }],
        &[],
    );

    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);

    assert_exit_code(code, 0);
    assert!(
        stdout.contains("`reads-the-screen` reads facts.screen.*, which needs the terminal host"),
        "{stdout}"
    );
}
