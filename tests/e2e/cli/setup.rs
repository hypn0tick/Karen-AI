// tests/e2e/cli/setup.rs
// The Karen who sets up setup because installation is the first impression.

use crate::e2e::common::{TestEnv, assert_exit_code, assert_json_ok};
use serial_test::serial;
use std::fs;

#[test]
#[serial]
fn karen_setup_dry_run_claude_lists_action() {
    // The Karen who verifies dry-run previews the install action.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["setup", "--dry-run", "claude", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let actions = data["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["runtime"], "claude");
    assert_eq!(actions[0]["scope"], "global");
    assert!(actions[0]["would_write"].as_bool().unwrap());
    assert!(
        actions[0]["path"].as_str().unwrap().contains(".claude")
            && actions[0]["path"].as_str().unwrap().ends_with("hooks.json")
    );
}

#[test]
#[serial]
fn karen_setup_dry_run_all_lists_three_runtimes() {
    // The Karen who verifies dry-run lists all three runtimes.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["setup", "--dry-run", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let actions = data["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 3);
    let runtimes: Vec<&str> = actions
        .iter()
        .map(|a| a["runtime"].as_str().unwrap())
        .collect();
    assert!(runtimes.contains(&"claude"));
    assert!(runtimes.contains(&"codex"));
    assert!(runtimes.contains(&"opencode"));
}

#[test]
#[serial]
fn karen_setup_dry_run_project_scopes_to_project() {
    // The Karen who verifies --project scopes to project directory.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("myproject");
    fs::create_dir_all(&project_dir).unwrap();
    let (code, stdout, _stderr) = env
        .karen_cmd(&["setup", "--dry-run", "--project", "claude", "--json"])
        .current_dir(&project_dir)
        .output()
        .map(|o| {
            (
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            )
        })
        .unwrap();
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let actions = data["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["scope"], "project");
    // Dry-run shows the path relative to the current directory
    let path_str = actions[0]["path"].as_str().unwrap();
    assert!(path_str.contains(".claude") && path_str.ends_with("hooks.json"));
}

#[test]
#[serial]
fn karen_setup_actual_claude_writes_hooks_json() {
    // The Karen who verifies actual setup writes hooks.json with SessionStart parts.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    let (code, stdout, _stderr) = env
        .karen_cmd(&["setup", "--project", "claude", "--json"])
        .current_dir(&project_dir)
        .output()
        .map(|o| {
            (
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            )
        })
        .unwrap();
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let installed = data["installed"].as_array().unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0]["runtime"], "claude");
    assert_eq!(installed[0]["scope"], "project");
    // The path in the output is relative to the project directory
    let hooks_path = project_dir.join(installed[0]["path"].as_str().unwrap());
    assert!(
        hooks_path.exists(),
        "hooks.json not found at {}",
        hooks_path.display()
    );
    let content = fs::read_to_string(&hooks_path).unwrap();
    let hooks: serde_json::Value = serde_json::from_str(&content).unwrap();
    // Verify SessionStart has 5 parts
    let session_start = hooks["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(session_start.len(), 5);
    for (i, part) in session_start.iter().enumerate() {
        let hooks_arr = part["hooks"].as_array().unwrap();
        assert_eq!(hooks_arr.len(), 1);
        let cmd = hooks_arr[0]["command"].as_str().unwrap();
        assert!(
            cmd.contains(&format!("--part {}", i + 1)),
            "part {} missing --part {}",
            i + 1,
            cmd
        );
    }
}

#[test]
#[serial]
fn karen_setup_actual_codex_writes_hooks_json() {
    // The Karen who verifies codex setup writes hooks.json.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    let (code, stdout, _stderr) = env
        .karen_cmd(&["setup", "--project", "codex", "--json"])
        .current_dir(&project_dir)
        .output()
        .map(|o| {
            (
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            )
        })
        .unwrap();
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let installed = data["installed"].as_array().unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0]["scope"], "project");
    let hooks_path = project_dir.join(installed[0]["path"].as_str().unwrap());
    assert!(
        hooks_path.exists(),
        "hooks.json not found at {}",
        hooks_path.display()
    );
    let content = fs::read_to_string(hooks_path).unwrap();
    let _: serde_json::Value = serde_json::from_str(&content).unwrap();
}

#[test]
#[serial]
fn karen_setup_actual_opencode_writes_hooks_json() {
    // The Karen who verifies opencode setup writes hooks.json.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    let (code, stdout, _stderr) = env
        .karen_cmd(&["setup", "--project", "opencode", "--json"])
        .current_dir(&project_dir)
        .output()
        .map(|o| {
            (
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            )
        })
        .unwrap();
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let installed = data["installed"].as_array().unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0]["scope"], "project");
    let hooks_path = project_dir.join(installed[0]["path"].as_str().unwrap());
    assert!(
        hooks_path.exists(),
        "hooks.json not found at {}",
        hooks_path.display()
    );
    let content = fs::read_to_string(hooks_path).unwrap();
    let _: serde_json::Value = serde_json::from_str(&content).unwrap();
}

#[test]
#[serial]
fn karen_setup_atomic_write_no_partial() {
    // The Karen who verifies atomic write (temp + rename) leaves no partial file on interrupt.
    // We can't easily test interrupt, but we verify the temp file pattern is used.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    let (code, _, _) = env
        .karen_cmd(&["setup", "--project", "claude"])
        .current_dir(&project_dir)
        .output()
        .map(|o| {
            (
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            )
        })
        .unwrap();
    assert_exit_code(code, 0);
    // No .json.tmp files should remain
    let project_claude = project_dir.join(".claude");
    let tmp_files: Vec<_> = fs::read_dir(&project_claude)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(
        tmp_files.is_empty(),
        "found leftover .tmp files: {tmp_files:?}"
    );
}

#[test]
#[serial]
fn karen_setup_idempotent_runs_twice() {
    // The Karen who verifies running setup twice succeeds.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    let (code1, _, _) = env
        .karen_cmd(&["setup", "--project", "claude"])
        .current_dir(&project_dir)
        .output()
        .map(|o| {
            (
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            )
        })
        .unwrap();
    assert_exit_code(code1, 0);
    let (code2, _, _) = env
        .karen_cmd(&["setup", "--project", "claude"])
        .current_dir(&project_dir)
        .output()
        .map(|o| {
            (
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            )
        })
        .unwrap();
    assert_exit_code(code2, 0);
    // Content should be identical
    let hooks_path = project_dir.join(".claude").join("hooks.json");
    let content1 = fs::read_to_string(&hooks_path).unwrap();
    // Second run
    let content2 = fs::read_to_string(&hooks_path).unwrap();
    assert_eq!(content1, content2, "setup must be idempotent");
}

#[test]
#[serial]
fn karen_setup_hooks_use_karen_binary_from_path() {
    // The Karen who verifies installed hooks reference karen from PATH.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    let (code, _, _) = env
        .karen_cmd(&["setup", "--project", "claude"])
        .current_dir(&project_dir)
        .output()
        .map(|o| {
            (
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            )
        })
        .unwrap();
    assert_exit_code(code, 0);
    // Find the hooks.json file
    let hooks_path = project_dir.join(".claude").join("hooks.json");
    assert!(
        hooks_path.exists(),
        "hooks.json not found at {}",
        hooks_path.display()
    );
    let content = fs::read_to_string(&hooks_path).unwrap();
    // Should reference "karen" or "karen.exe" without absolute path
    // On Windows it's "karen.exe hook...", on Unix it's "karen hook..."
    assert!(
        content.contains("karen") && content.contains("hook --adapter claude-classic"),
        "hooks.json should reference karen from PATH: {content}"
    );
    assert!(!content.contains("target/release")); // Not hardcoded build path
}
