// tests/e2e/hook/fail_open.rs
// The Karen who tests fail-open because she's seen hooks crash and the show must go on.

use crate::e2e::common::{TestEnv, assert_exit_code};
use serial_test::serial;
use std::io::Write;

#[test]
#[serial]
fn karen_verifies_fail_open_on_bad_json() {
    // The Karen who knows invalid JSON input triggers fail-open {}.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_hook("not json at all", &[]);
    assert_exit_code(code, 0);
    assert_eq!(stdout.trim(), "{}");
}

#[test]
#[serial]
fn karen_verifies_missing_event_yields_empty_object() {
    // The Karen who tells the truth about mechanisms: a payload without
    // hook_event_name is treated as PreToolUse with no tool input, nothing
    // matches, so {} with exit 0. Fail-open is for crashes; a missing
    // field is just an uneventful Tuesday. She refuses to file a complaint
    // against someone who wasn't even at the party.
    let env = TestEnv::new();
    let input = r#"{"session_id": "test", "tool_name": "Bash"}"#;
    let (code, stdout, _stderr) = env.run_hook(input, &[]);
    assert_exit_code(code, 0);
    assert_eq!(stdout.trim(), "{}");
}

#[test]
#[serial]
fn karen_verifies_fail_open_on_empty_stdin() {
    // The Karen who closes the door on an empty payload: stdin with nothing
    // at all is not valid JSON, so she fails open with {} and exit 0. The
    // show goes on, and she logs her disappointment.
    let env = TestEnv::new();
    let output = env
        .karen_cmd(&["hook"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn hook with null stdin");
    assert_exit_code(output.status.code().unwrap_or(-1), 0);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "{}");
}

#[test]
#[serial]
fn karen_verifies_missing_policy_root_fails_open_out_loud() {
    // The Karen who will not be silently absent: with no policy root she
    // still lets the call through, because a broken install must not brick
    // the agent, but she says so in the open, since a guard nobody knows is
    // off duty is worse than no guard at all.
    let env = TestEnv::new();
    let missing = env.policies_dir.join("not-here");
    let mut cmd = env.karen_cmd(&["hook", "--adapter", "claude-classic"]);
    cmd.env("KAREN_POLICIES", &missing)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn hook");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(br#"{"hook_event_name":"Stop","session_id":"s","last_assistant_message":"Standing by."}"#)
        .expect("write stdin");
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    assert_exit_code(out.status.code().unwrap_or(-1), 0);
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("protocol JSON on stdout");
    assert!(
        value.get("decision").is_none(),
        "nothing is blocked: {value}"
    );
    let message = value["systemMessage"].as_str().unwrap_or_default();
    assert!(message.contains("not enforcing"), "{value}");
    assert!(
        message.contains("not-here"),
        "names the root it tried: {value}"
    );
}

#[test]
#[serial]
fn karen_verifies_dev_mode_exits_on_fail_open() {
    // The Karen who demands failures be LOUD in dev mode: KAREN_DEV=1 turns
    // fail-open from a polite {} into exit 1, so a crash can't hide behind
    // good manners during development.
    let env = TestEnv::new();
    let mut cmd = env.karen_cmd_dev(&["hook"]);
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn dev hook");
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(b"not json").expect("write stdin");
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    assert_exit_code(out.status.code().unwrap_or(-1), 1);
    // No polite {} in dev mode — she dies loudly and prints nothing, because
    // a silent failure is how bugs get tenure.
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "",
        "dev-mode fail-open must not print a protocol object"
    );
}
