// tests/e2e/hook/sessionstart.rs
// The Karen who tests SessionStart parts because rule injection is the whole point.

use crate::e2e::common::{TestEnv, assert_exit_code, assert_hook_sessionstart_part};
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn karen_verifies_sessionstart_parts_1_to_4_have_additional_context() {
    // The Karen who pads the rule set past three chunk boundaries, because a
    // one-part policy proves nothing about part 2. She demands material.
    let env = TestEnv::new();
    for i in 1..=4 {
        let padding = format!(
            "# Padding rule {i}\n\n{}\n",
            "The Karen who fills parts so the chunker has something to chunk. ".repeat(110)
        );
        std::fs::write(
            env.policies_dir
                .join("rules")
                .join(format!("99-padding-{i:02}.md")),
            padding,
        )
        .expect("write padding rule");
    }
    let input = json!({
        "hook_event_name": "SessionStart",
        "session_id": format!("test-parts-{}", std::process::id()),
        "source": "startup"
    });
    for part in 1..=4 {
        // The Karen who checks every part carries its own slice of the rules.
        let (code, stdout, _stderr) =
            env.run_hook(&input.to_string(), &["--part", &part.to_string()]);
        assert_exit_code(code, 0);
        assert_hook_sessionstart_part(&stdout, part);
    }
}

#[test]
#[serial]
fn karen_verifies_sessionstart_part_5_is_empty() {
    // The Karen who verifies part 5 (beyond packed count) emits empty object.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "SessionStart",
        "session_id": "test-part-5",
        "source": "startup"
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &["--part", "5"]);
    assert_exit_code(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        out,
        serde_json::json!({}),
        "part 5 should be empty: {stdout}"
    );
}

#[test]
#[serial]
fn karen_verifies_sessionstart_part_99_is_empty() {
    // The Karen who verifies part beyond count emits empty object.
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "SessionStart",
        "session_id": "test-part-99",
        "source": "startup"
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &["--part", "99"]);
    assert_exit_code(code, 0);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        out,
        serde_json::json!({}),
        "part 99 should be empty: {stdout}"
    );
}

#[test]
#[serial]
fn karen_verifies_sessionstart_re_delivery_suppressed() {
    // The Karen who verifies re-delivery is suppressed within same session.
    let env = TestEnv::new();
    let session_id = format!("test-redelivery-{}", std::process::id());
    let input = json!({
        "hook_event_name": "SessionStart",
        "session_id": session_id,
        "source": "startup"
    });
    // First delivery - part 1
    let (code1, stdout1, _) = env.run_hook(&input.to_string(), &["--part", "1"]);
    assert_exit_code(code1, 0);
    assert_hook_sessionstart_part(&stdout1, 1);

    // Second delivery - same session, part 1 (should be suppressed)
    let (code2, stdout2, _) = env.run_hook(&input.to_string(), &["--part", "1"]);
    assert_exit_code(code2, 0);
    let out2: serde_json::Value = serde_json::from_str(&stdout2).unwrap();
    // Should be empty because already delivered
    assert_eq!(
        out2,
        serde_json::json!({}),
        "re-delivery should be suppressed: {stdout2}"
    );
}

#[test]
#[serial]
fn karen_verifies_sessionstart_part_0_treated_as_none() {
    // The Karen who verifies part 0 is treated as None (all parts).
    let env = TestEnv::new();
    let input = json!({
        "hook_event_name": "SessionStart",
        "session_id": "test-part-0",
        "source": "startup"
    });
    let (code, stdout, _stderr) = env.run_hook(&input.to_string(), &["--part", "0"]);
    assert_exit_code(code, 0);
    // Part 0 is not a valid part number (1-based); the injector treats it as
    // "nothing to deliver" and the protocol stays silent. The Karen who
    // doesn't crash on your creative numbering — she just refuses to chunk it.
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        out,
        serde_json::json!({}),
        "part 0 must deliver nothing: {stdout}"
    );
}
