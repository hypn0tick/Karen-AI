// tests/e2e/facts/context.rs
// The Karen who tests context occupancy: what a transcript says about how full
// the window is, and what she refuses to say when it says nothing.

use crate::e2e::common::{
    DEFAULT_CONFIG, GateFixture, TestEnv, assert_evaluate_action, assert_exit_code,
    assert_hook_deny, assert_json_ok,
};
use serde_json::json;
use serial_test::serial;

/// A config that answers a runtime's window: `[facts.context_window]` names
/// the runtime, so one runtime's number never answers for another's model.
const WINDOW_CONFIG: &str = r#"# Personal Rules engine configuration.
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

[facts.context_window]
claude = 10000
codex = 10000
"#;

/// A Claude Code session transcript: an older assistant turn, then the latest
/// one she must read the occupancy from.
const CLAUDE_TRANSCRIPT: &str = concat!(
    r#"{"type":"user","message":{"content":"hi"}}"#,
    "\n",
    r#"{"type":"assistant","message":{"model":"m","usage":{"input_tokens":999,"output_tokens":1}}}"#,
    "\n",
    r#"{"type":"user","message":{"content":"and then"}}"#,
    "\n",
    r#"{"type":"assistant","message":{"model":"m","usage":{"input_tokens":200,"cache_read_input_tokens":4000,"cache_creation_input_tokens":136,"output_tokens":50}}}"#,
    "\n",
);

/// 4336 = 200 + 4000 + 136: input plus both cache counts of the latest turn,
/// over a configured 10000-token window.
const LATEST_TOKENS: u64 = 4336;
const LATEST_RATIO: f64 = 0.4336;

/// A Codex rollout: a context line, then the token-count event she reads.
const CODEX_ROLLOUT: &str = concat!(
    r#"{"timestamp":"t0","type":"session_meta"}"#,
    "\n",
    r#"{"timestamp":"t1","type":"turn_context","payload":{"model":"gpt-6-luna"}}"#,
    "\n",
    r#"{"timestamp":"t2","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{},"last_token_usage":{"input_tokens":100,"cached_input_tokens":200,"cache_write_input_tokens":300,"output_tokens":5},"model_context_window":258400}}}"#,
    "\n",
);

/// A gate that matches only when `field` equals `value`. Test values leak:
/// a fixture lives as long as the process that loads it.
fn equals_gate(
    file: &'static str,
    id: &str,
    field: &str,
    value: impl serde::Serialize,
) -> GateFixture {
    let content: &'static str = Box::leak(
        format!(
            r#"{{
  "id": "{id}",
  "description": "The test gate for {field}.",
  "event": "PreToolUse",
  "match": {{ "all": [ {{ "type": "field_equals", "field": "{field}", "value": {} }} ] }},
  "action": {{ "type": "deny", "reason": "occupancy matched" }}
}}"#,
            serde_json::to_string(&value).unwrap_or_default()
        )
        .into_boxed_str(),
    );
    GateFixture { file, content }
}

fn claude_input(transcript: Option<&std::path::Path>) -> String {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": "facts-claude",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "transcript_path": transcript.map(|p| p.to_string_lossy()).unwrap_or_default(),
    })
    .to_string()
}

fn codex_input(rollout: Option<&std::path::Path>) -> String {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": "facts-codex",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "transcript_path": rollout.map(|p| p.to_string_lossy()).unwrap_or_default(),
    })
    .to_string()
}

/// She evaluates the payload as the named runtime and returns the verdict JSON.
fn evaluate(env: &TestEnv, input: &str, runtime: &str) -> serde_json::Value {
    let (code, stdout, _stderr) = env.run_evaluate_stdin(input, &["--json", "--runtime", runtime]);
    assert_exit_code(code, 0);
    assert_json_ok(&stdout)
}

#[test]
#[serial]
fn karen_reads_claude_occupancy_from_the_transcript_tail() {
    // The Karen who reads only the tail still finds the latest turn: 200 in,
    // 4000 cached read, 136 cached written, and never the older turn's 999.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[equals_gate(
            "claude-tokens.json",
            "claude-tokens",
            "facts.context_tokens",
            LATEST_TOKENS,
        )],
        &[],
    );
    let transcript = env.temp_dir.path().join("transcript.jsonl");
    std::fs::write(&transcript, CLAUDE_TRANSCRIPT).expect("write transcript");
    let data = evaluate(&env, &claude_input(Some(&transcript)), "claude");
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_reports_claude_occupancy_absent_without_a_transcript() {
    // No transcript she can read: the fact is absent, so a gate matching on
    // any value stays quiet. Absence is not zero.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[equals_gate(
            "claude-absent.json",
            "claude-absent",
            "facts.context_tokens",
            0,
        )],
        &[],
    );
    let data = evaluate(&env, &claude_input(None), "claude");
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_reports_claude_occupancy_absent_on_an_unreadable_transcript() {
    // A compressed transcript is not a transcript she can read: no invented
    // number, no matching gate. The zstd magic bytes stand in for one.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[equals_gate(
            "claude-zst.json",
            "claude-zst",
            "facts.context_tokens",
            LATEST_TOKENS,
        )],
        &[],
    );
    let transcript = env.temp_dir.path().join("rollout.jsonl.zst");
    std::fs::write(&transcript, [0x28, 0xb5, 0x2f, 0xfd, 0x00, 0x01]).expect("write zstd bytes");
    let data = evaluate(&env, &claude_input(Some(&transcript)), "claude");
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_ratios_claude_occupancy_against_the_configured_window() {
    // 4336 tokens in a 10000-token window: a ratio gate names what it means,
    // and she reads both facts it divides to say it.
    let env = TestEnv::with_config(
        WINDOW_CONFIG,
        &[equals_gate(
            "claude-ratio.json",
            "claude-ratio",
            "facts.context_ratio",
            LATEST_RATIO,
        )],
        &[],
    );
    let transcript = env.temp_dir.path().join("transcript.jsonl");
    std::fs::write(&transcript, CLAUDE_TRANSCRIPT).expect("write transcript");
    let data = evaluate(&env, &claude_input(Some(&transcript)), "claude");
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_leaves_the_ratio_absent_without_a_window() {
    // Claude Code does not report its window and this config does not answer
    // for one: no ratio, and a gate on it stays quiet. Tokens still count.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[equals_gate(
            "claude-no-ratio.json",
            "claude-no-ratio",
            "facts.context_ratio",
            LATEST_RATIO,
        )],
        &[],
    );
    let transcript = env.temp_dir.path().join("transcript.jsonl");
    std::fs::write(&transcript, CLAUDE_TRANSCRIPT).expect("write transcript");
    let data = evaluate(&env, &claude_input(Some(&transcript)), "claude");
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_reads_codex_occupancy_tokens_from_the_rollout() {
    // The rollout's own `token_count` answers: 100 + 200 + 300 tokens. One gate
    // in the env, so only the tokens can be what denies.
    let env = TestEnv::with_config(
        WINDOW_CONFIG,
        &[equals_gate(
            "codex-tokens.json",
            "codex-tokens",
            "facts.context_tokens",
            600,
        )],
        &[],
    );
    let rollout = env.temp_dir.path().join("rollout.jsonl");
    std::fs::write(&rollout, CODEX_ROLLOUT).expect("write rollout");
    let data = evaluate(&env, &codex_input(Some(&rollout)), "codex");
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_prefers_the_rollouts_codex_window_over_the_configured_one() {
    // The rollout says 258400 and the config says 10000 for codex. The only
    // gate in the env matches 258400 alone, so the verdict is deny only when
    // the rollout wins: a configured-first order answers 10000 and allows.
    let env = TestEnv::with_config(
        WINDOW_CONFIG,
        &[equals_gate(
            "codex-window.json",
            "codex-window",
            "facts.context_window",
            258_400,
        )],
        &[],
    );
    let rollout = env.temp_dir.path().join("rollout.jsonl");
    std::fs::write(&rollout, CODEX_ROLLOUT).expect("write rollout");
    // Through the hook, where the runtime's calls actually arrive.
    let (code, stdout, _) = env.run_hook(&codex_input(Some(&rollout)), &["--runtime", "codex"]);
    assert_exit_code(code, 0);
    assert_hook_deny(&stdout, Some("occupancy matched"));
}

#[test]
#[serial]
fn karen_answers_a_configured_codex_window_only_when_unreadable() {
    // A rollout she cannot read leaves the window to the config: 10000
    // answers for codex only because the rollout did not answer at all.
    let env = TestEnv::with_config(
        WINDOW_CONFIG,
        &[equals_gate(
            "codex-config-window.json",
            "codex-config-window",
            "facts.context_window",
            10_000,
        )],
        &[],
    );
    let rollout = env.temp_dir.path().join("rollout-closed.jsonl.zst");
    std::fs::write(&rollout, [0x28, 0xb5, 0x2f, 0xfd, 0x00, 0x01]).expect("write zstd bytes");
    let data = evaluate(&env, &codex_input(Some(&rollout)), "codex");
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_reports_codex_occupancy_absent_without_a_rollout() {
    let env = TestEnv::with_config(
        WINDOW_CONFIG,
        &[equals_gate(
            "codex-absent.json",
            "codex-absent",
            "facts.context_tokens",
            0,
        )],
        &[],
    );
    let data = evaluate(&env, &codex_input(None), "codex");
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_honors_a_runtime_supplied_occupancy_over_her_own_reading() {
    // The bridge supplies the facts: 500 tokens in a 1000-token window is a
    // ratio of one half, and no transcript she could read overrides it.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[equals_gate(
            "supplied-ratio.json",
            "supplied-ratio",
            "facts.context_ratio",
            0.5,
        )],
        &[],
    );
    let input = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "facts-supplied",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "facts": { "context_tokens": 500, "context_window": 1000 },
    })
    .to_string();
    let data = evaluate(&env, &input, "opencode");
    assert_evaluate_action(&data, "deny");
}

/// `claude = 0` configures a window of nothing: a number she must not divide by.
const ZERO_WINDOW_CONFIG: &str = r#"# Personal Rules engine configuration.
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

[facts.context_window]
claude = 0
"#;

/// A gate any non-negative ratio would trip: the only thing that keeps it
/// quiet is a ratio that is not there.
const ANY_RATIO_GATE: &str = r#"{
  "id": "any-ratio",
  "description": "Matches whenever a context ratio exists at all.",
  "event": "PreToolUse",
  "match": { "all": [ { "type": "field_gte", "field": "facts.context_ratio", "value": 0 } ] },
  "action": { "type": "deny", "reason": "a ratio exists" }
}"#;

#[test]
#[serial]
fn karen_leaves_the_ratio_absent_when_the_window_is_zero() {
    // A configured window of zero is no window: no ratio, and a ratio gate
    // stays quiet instead of firing on a number made of nothing. The division
    // guard itself is pinned by a unit test, since a non-finite ratio would
    // serialise as null and stay quiet here either way.
    let env = TestEnv::with_config(
        ZERO_WINDOW_CONFIG,
        &[GateFixture {
            file: "any-ratio.json",
            content: ANY_RATIO_GATE,
        }],
        &[],
    );
    let transcript = env.temp_dir.path().join("transcript.jsonl");
    std::fs::write(&transcript, CLAUDE_TRANSCRIPT).expect("write transcript");
    let data = evaluate(&env, &claude_input(Some(&transcript)), "claude");
    assert_evaluate_action(&data, "allow");
}
