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
            && actions[0]["path"]
                .as_str()
                .unwrap()
                .ends_with("settings.json")
    );
    assert_eq!(actions[0]["mode"], "merge");
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
    assert!(path_str.contains(".claude") && path_str.ends_with("settings.json"));
}

#[test]
#[serial]
fn karen_setup_actual_claude_writes_settings_json() {
    // The Karen who verifies setup registers her hooks in settings.json, the
    // file Claude Code actually reads, with all five SessionStart parts.
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
        "settings.json not found at {}",
        hooks_path.display()
    );
    assert!(hooks_path.ends_with("settings.json"));
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
fn karen_setup_actual_opencode_writes_plugin() {
    // The Karen who verifies opencode setup installs the bridge plugin, since
    // OpenCode loads plugins and never reads a hooks.json.
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
    let plugin_path = project_dir.join(installed[0]["path"].as_str().unwrap());
    assert!(
        plugin_path.ends_with(".opencode/plugins/karen-ai.js"),
        "unexpected plugin path {}",
        plugin_path.display()
    );
    let content = fs::read_to_string(plugin_path).unwrap();
    assert!(content.contains("\"tool.execute.before\""));
    assert!(content.contains("\"tool.execute.after\""));
    assert!(content.contains("\"experimental.chat.system.transform\""));
    assert!(!project_dir.join(".opencode").join("hooks.json").exists());
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
    // The Karen who verifies a second setup changes nothing: same bytes,
    // no second copy of herself.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    let settings = project_dir.join(".claude").join("settings.json");
    assert_exit_code(run_setup_claude(&env, &project_dir), 0);
    let content1 = fs::read_to_string(&settings).unwrap();
    assert_exit_code(run_setup_claude(&env, &project_dir), 0);
    let content2 = fs::read_to_string(&settings).unwrap();
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
    let hooks_path = project_dir.join(".claude").join("settings.json");
    assert!(
        hooks_path.exists(),
        "settings.json not found at {}",
        hooks_path.display()
    );
    let content = fs::read_to_string(&hooks_path).unwrap();
    // Should reference "karen" or "karen.exe" without absolute path
    // On Windows it's "karen.exe hook...", on Unix it's "karen hook..."
    assert!(
        content.contains("karen") && content.contains("hook --adapter claude-classic"),
        "settings.json should reference karen from PATH: {content}"
    );
    assert!(!content.contains("target/release")); // Not hardcoded build path
}

fn run_setup_claude(env: &TestEnv, project_dir: &std::path::Path) -> i32 {
    env.karen_cmd(&["setup", "--project", "claude"])
        .current_dir(project_dir)
        .output()
        .map(|o| o.status.code().unwrap_or(-1))
        .unwrap()
}

fn karen_commands(settings: &serde_json::Value) -> Vec<String> {
    settings["hooks"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|groups| groups.as_array().unwrap().iter())
        .flat_map(|group| group["hooks"].as_array().unwrap().iter())
        .filter_map(|hook| hook["command"].as_str())
        .filter(|command| command.contains("karen") && command.contains(" hook"))
        .map(str::to_string)
        .collect()
}

#[test]
#[serial]
fn karen_setup_claude_merge_preserves_foreign_settings_and_hooks() {
    // The Karen who moves in without rearranging anyone else's furniture:
    // foreign keys, foreign hooks and foreign groups survive her install.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    let claude_dir = project_dir.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join("settings.json"),
        r#"{
  "model": "opus",
  "permissions": { "allow": ["Bash(git status)"] },
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-linter --strict" }] }
    ],
    "Notification": [
      { "hooks": [{ "type": "command", "command": "notify-send done" }] }
    ]
  }
}"#,
    )
    .unwrap();

    assert_exit_code(run_setup_claude(&env, &project_dir), 0);

    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["permissions"]["allow"][0], "Bash(git status)");
    let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(pre[0]["matcher"], "Bash");
    assert_eq!(pre[0]["hooks"][0]["command"], "my-linter --strict");
    assert!(
        pre.iter()
            .any(|g| g["hooks"][0]["command"].as_str().unwrap().contains("karen")),
        "karen PreToolUse hook missing: {pre:?}"
    );
    assert_eq!(
        settings["hooks"]["Notification"][0]["hooks"][0]["command"],
        "notify-send done"
    );
    assert_eq!(
        settings["hooks"]["SessionStart"].as_array().unwrap().len(),
        5
    );
}

#[test]
#[serial]
fn karen_setup_claude_replaces_stale_karen_entries_without_duplicates() {
    // The Karen who will not share a settings file with an older copy of
    // herself: stale entries go, current ones arrive exactly once, and a
    // foreign hook sharing a group with her keeps its seat.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    let claude_dir = project_dir.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join("settings.json"),
        r#"{
  "hooks": {
    "PreToolUse": [
      { "hooks": [
        { "type": "command", "command": "\"C:/old/bin/karen.exe\" hook --adapter claude", "timeout": 3 },
        { "type": "command", "command": "audit-log" }
      ] }
    ],
    "Notification": [
      { "hooks": [{ "type": "command", "command": "karen hook --adapter claude-classic" }] }
    ]
  }
}"#,
    )
    .unwrap();

    assert_exit_code(run_setup_claude(&env, &project_dir), 0);
    let first: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.json")).unwrap())
            .unwrap();
    assert_exit_code(run_setup_claude(&env, &project_dir), 0);
    let second: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.json")).unwrap())
            .unwrap();

    assert_eq!(first, second, "a second setup must not change anything");
    let commands = karen_commands(&second);
    assert!(
        !commands.iter().any(|c| c.contains("C:/old/bin")),
        "stale karen entry survived: {commands:?}"
    );
    assert!(
        second["hooks"].get("Notification").is_none(),
        "stale karen-only event survived"
    );
    let mut unique = commands.clone();
    unique.sort();
    unique.dedup();
    // SessionStart carries five distinct parts; every other event exactly one.
    let events = second["hooks"].as_object().unwrap();
    for (event, groups) in events {
        let karen_groups = groups
            .as_array()
            .unwrap()
            .iter()
            .filter(|g| {
                g["hooks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|h| h["command"].as_str().unwrap().contains("karen"))
            })
            .count();
        let expected = if event == "SessionStart" { 5 } else { 1 };
        assert_eq!(
            karen_groups, expected,
            "{event} has {karen_groups} karen groups"
        );
    }
    let pre = second["hooks"]["PreToolUse"].as_array().unwrap();
    assert!(
        pre.iter().any(|g| g["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["command"] == "audit-log")),
        "foreign hook sharing a group with karen was lost: {pre:?}"
    );
}

#[test]
#[serial]
fn karen_setup_claude_refuses_invalid_settings_json() {
    // The Karen who does not "fix" other people's files: unreadable JSON is
    // refused and left exactly as she found it.
    let env = TestEnv::new();
    let project_dir = env.temp_dir.path().join("project");
    let claude_dir = project_dir.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let broken = "{ \"model\": \"opus\", // a comment Claude Code would reject\n";
    fs::write(claude_dir.join("settings.json"), broken).unwrap();

    let code = run_setup_claude(&env, &project_dir);

    assert_ne!(code, 0, "setup must fail on invalid settings.json");
    assert_eq!(
        fs::read_to_string(claude_dir.join("settings.json")).unwrap(),
        broken
    );
}

#[test]
#[serial]
fn karen_setup_removes_only_her_own_legacy_hooks_json() {
    // The Karen who clears her own dead hooks.json from both runtimes, and
    // leaves a stranger's hooks.json alone however dead it is.
    let env = TestEnv::new();
    let legacy = r#"{ "_comment": "old Karen-AI install", "hooks": {
  "PreToolUse": [ { "hooks": [ { "type": "command", "command": "karen.exe hook --adapter claude-classic" } ] } ] } }"#;
    let foreign = r#"{ "hooks": {
  "PreToolUse": [ { "hooks": [ { "type": "command", "command": "someone-elses-tool" } ] } ] } }"#;

    let mine = env.temp_dir.path().join("mine");
    fs::create_dir_all(mine.join(".claude")).unwrap();
    fs::create_dir_all(mine.join(".opencode")).unwrap();
    fs::write(mine.join(".claude").join("hooks.json"), legacy).unwrap();
    fs::write(mine.join(".opencode").join("hooks.json"), legacy).unwrap();

    let theirs = env.temp_dir.path().join("theirs");
    fs::create_dir_all(theirs.join(".claude")).unwrap();
    fs::write(theirs.join(".claude").join("hooks.json"), foreign).unwrap();

    for (dir, runtime) in [(&mine, "claude"), (&mine, "opencode"), (&theirs, "claude")] {
        let code = env
            .karen_cmd(&["setup", "--project", runtime])
            .current_dir(dir)
            .output()
            .map(|o| o.status.code().unwrap_or(-1))
            .unwrap();
        assert_exit_code(code, 0);
    }

    assert!(!mine.join(".claude").join("hooks.json").exists());
    assert!(!mine.join(".opencode").join("hooks.json").exists());
    assert_eq!(
        fs::read_to_string(theirs.join(".claude").join("hooks.json")).unwrap(),
        foreign
    );
}
