// tests/e2e/state/latch.rs
// The Karen who tests the latch: a gate that fires once, stays quiet while the
// window is still full, and only fires again once it has actually emptied.

use crate::e2e::common::{
    GateFixture, TestEnv, assert_exit_code, assert_hook_allow, assert_hook_deny,
};
use serde_json::json;
use serial_test::serial;

/// File backend: the latch survives between hook processes, like the session does.
const FILE_CONFIG: &str = r#"# Personal Rules engine configuration.
[engine]
fail_open = true
log_level = "warn"
timeout_ms = 5000

[policy]
rules_dir = "rules"
gates_dir = "gates"
validate_on_load = true

[state]
backend = "file"
file_dir = "~/.cache/karen"
"#;

/// A gate that holds a session at 40% occupancy, and re-arms below 30%: the
/// latch's `field` names the fact the gate watches, so the fact is collected.
const LATCH_GATE: &str = r#"{
  "id": "occupancy-latch",
  "description": "Hold once at 40% context, again only after it dropped below 30%.",
  "event": "PreToolUse",
  "match": {
    "all": [
      { "type": "field_gte", "field": "facts.context_ratio", "value": 0.4 }
    ]
  },
  "action": {
    "type": "deny",
    "reason": "context is 40% full: compress the finished work, then try again"
  },
  "state": {
    "key": "latch:occupancy",
    "field": "facts.context_ratio",
    "rearm_below": 0.3
  }
}"#;

/// The same gate, but its latch forgets on compaction.
const RESET_LATCH_GATE: &str = r#"{
  "id": "occupancy-reset-latch",
  "description": "Hold once at 40% context, again only after a compaction or a drop below 30%.",
  "event": "PreToolUse",
  "match": {
    "all": [
      { "type": "field_gte", "field": "facts.context_ratio", "value": 0.4 }
    ]
  },
  "action": {
    "type": "deny",
    "reason": "context is 40% full: compress the finished work, then try again"
  },
  "state": {
    "key": "latch:reset-occupancy",
    "field": "facts.context_ratio",
    "rearm_below": 0.3,
    "reset_on_compact": true
  }
}"#;

/// A `PreToolUse` payload whose bridge supplied the ratio, as the `OpenCode`
/// bridge supplies it: the latch watches what the payload says.
fn call(ratio: f64) -> String {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": "latch-session",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "facts": { "runtime": "opencode", "context_ratio": ratio },
    })
    .to_string()
}

fn env(gate: &'static str) -> TestEnv {
    TestEnv::with_config(
        FILE_CONFIG,
        &[GateFixture {
            file: "latch.json",
            content: gate,
        }],
        &[],
    )
}

#[test]
#[serial]
fn latch_fires_once_and_rearms_only_below_the_line() {
    // The Karen who trips the breaker at 45%, ignores you at 50% (it already
    // tripped), resets it at 25% (below the line, without matching), and trips
    // again at 42%: one hold per filling, never a hold per call.
    let env = env(LATCH_GATE);
    let (_, fired, _) = env.run_hook(&call(0.45), &[]);
    assert_hook_deny(&fired, Some("40% full"));
    // Latched, and 50% is not below the line: quiet, not re-fired.
    let (_, latched, _) = env.run_hook(&call(0.50), &[]);
    assert_hook_allow(&latched);
    // 25% is below the line: the latch re-arms, and a full window never
    // matches, so the gate stays quiet while it does.
    let (_, rearmed, _) = env.run_hook(&call(0.25), &[]);
    assert_hook_allow(&rearmed);
    // Back above the bar with the latch open: it fires again.
    let (_, again, _) = env.run_hook(&call(0.42), &[]);
    assert_hook_deny(&again, Some("40% full"));
}

#[test]
#[serial]
fn latch_holds_through_a_drop_and_a_climb_without_rearming() {
    // 42% re-arms the latch; 28% re-arms it too (below the line, quiet);
    // 42% again would fire — but a climb that never dipped stays latched.
    let env = env(LATCH_GATE);
    let (_, fired, _) = env.run_hook(&call(0.42), &[]);
    assert_hook_deny(&fired, Some("40% full"));
    // A climb to 50% never dips below the line: the latch holds, no fire.
    let (_, held, _) = env.run_hook(&call(0.50), &[]);
    assert_hook_allow(&held);
    let (_, held_again, _) = env.run_hook(&call(0.45), &[]);
    assert_hook_allow(&held_again);
    // Only an actual dip below the line re-arms the gate.
    let (_, dipped, _) = env.run_hook(&call(0.28), &[]);
    assert_hook_allow(&dipped);
    let (_, again, _) = env.run_hook(&call(0.45), &[]);
    assert_hook_deny(&again, Some("40% full"));
}

#[test]
#[serial]
fn latch_rearms_on_a_missing_field_rather_than_firing_forever() {
    // No ratio in the payload: nothing to watch, nothing to compare, and
    // nothing to re-arm on. The gate cannot match (the fact is absent), and
    // the latch is never armed by a gate that never fires.
    let env = env(LATCH_GATE);
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "latch-session",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "facts": { "runtime": "opencode" },
    })
    .to_string();
    let (code, quiet, _) = env.run_hook(&input, &[]);
    assert_exit_code(code, 0);
    assert_hook_allow(&quiet);
    // The latch still works when the ratio comes back.
    let (_, fired, _) = env.run_hook(&call(0.41), &[]);
    assert_hook_deny(&fired, Some("40% full"));
}

#[test]
#[serial]
fn latch_forgets_on_compaction_and_fires_again() {
    // A latch with `reset_on_compact` holds once, and a compaction — the
    // thing that empties the window — is as good as a dip below the line.
    let env = env(RESET_LATCH_GATE);
    let (_, fired, _) = env.run_hook(&call(0.42), &[]);
    assert_hook_deny(&fired, Some("40% full"));
    let (_, held, _) = env.run_hook(&call(0.45), &[]);
    assert_hook_allow(&held);
    let compact = json!({ "hook_event_name": "PostCompact", "session_id": "latch-session" });
    let (code, _, _) = env.run_hook(&compact.to_string(), &[]);
    assert_exit_code(code, 0);
    // After the compaction the latch is open again, so the gate fires again.
    let (_, again, _) = env.run_hook(&call(0.45), &[]);
    assert_hook_deny(&again, Some("40% full"));
}

#[test]
#[serial]
fn a_latch_without_the_line_never_rearms() {
    // `field` without `rearm_below` is not a latch: the state spec is a strike
    // counter's, so the gate fires on every matching call.
    let gate = r#"{
  "id": "lineless",
  "description": "Not a latch: no field to watch, so no holding.",
  "event": "PreToolUse",
  "match": {
    "all": [
      { "type": "field_gte", "field": "facts.context_ratio", "value": 0.4 }
    ]
  },
  "action": { "type": "deny", "reason": "lineless fired" },
  "state": { "key": "lineless", "rearm_below": 0.3 }
}"#;
    let env = TestEnv::with_config(
        FILE_CONFIG,
        &[GateFixture {
            file: "lineless.json",
            content: gate,
        }],
        &[],
    );
    for _ in 0..2 {
        let (_, fired, _) = env.run_hook(&call(0.45), &[]);
        assert_hook_deny(&fired, Some("lineless fired"));
    }
}
