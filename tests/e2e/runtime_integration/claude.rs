// tests/e2e/runtime_integration/claude.rs
// The Karen who tests real Claude Code integration because mocks are for
// people who don't want to know the truth.

use crate::e2e::common::{
    TestEnv, assert_exit_code, assert_json_ok, run_command, runtime_available,
};
use serial_test::serial;
use std::fs;

#[test]
#[serial]
fn karen_runtime_claude_install_and_runtime_responds() {
    // The Karen who checks the guest list first: no claude CLI, no test, no
    // drama. When it IS installed, she verifies the wiring it will actually
    // read — because a hooks.json the runtime ignores is a decoration, not
    // a guardrail.
    if !runtime_available("claude") {
        eprintln!("skipping: claude CLI not installed");
        return;
    }
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("claude-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let mut install = env.karen_cmd(&["setup", "claude", "--project", "--json"]);
    install.current_dir(&project_dir);
    let (code, stdout, _stderr) = run_command(install);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let installed = data["installed"].as_array().unwrap();
    assert_eq!(installed.len(), 1);
    // The Karen who reads the wiring before she trusts it.
    let hooks_path = project_dir.join(installed[0]["path"].as_str().unwrap());
    let content = fs::read_to_string(&hooks_path).unwrap();
    let hooks: serde_json::Value = serde_json::from_str(&content).unwrap();
    let parts = hooks["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(parts.len(), 5, "SessionStart must register 5 parts");
    for (i, part) in parts.iter().enumerate() {
        let cmd = part["hooks"][0]["command"].as_str().unwrap();
        assert!(
            cmd.contains(&format!("--part {}", i + 1)),
            "part {} wiring missing --part {}: {}",
            i + 1,
            i + 1,
            cmd
        );
    }
}
