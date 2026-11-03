// tests/e2e/facts/provider_error.rs
// The Karen who tests what the provider itself broke: the fact that says the
// pipe failed, not the agent, and stays absent once a later call gets through.

use crate::e2e::common::{
    DEFAULT_CONFIG, GateFixture, TestEnv, assert_evaluate_action, assert_exit_code, assert_json_ok,
};
use serde_json::json;
use serial_test::serial;

/// A Claude Code transcript whose last assistant entry is the API error.
const CLAUDE_ERROR_LAST: &str = concat!(
    r#"{"type":"user","message":{"content":"go"}}"#,
    "\n",
    r#"{"type":"assistant","message":{"model":"real","usage":{"input_tokens":10,"output_tokens":2}}}"#,
    "\n",
    r#"{"type":"assistant","isApiErrorMessage":true,"message":{"model":"<synthetic>","content":[{"type":"text","text":"API Error: The operation timed out"}]}}"#,
    "\n",
);

/// The same transcript, then a successful assistant entry after the error.
fn claude_error_recovered() -> String {
    format!(
        "{CLAUDE_ERROR_LAST}{}\n",
        r#"{"type":"assistant","message":{"model":"real","usage":{"input_tokens":3,"output_tokens":4}}}"#
    )
}

/// A Codex rollout whose last relevant line is the error event. Shape per the
/// codex-rs source: `EventMsg::Error(ErrorEvent)` behind `RolloutItem::EventMsg`,
/// `tag = "type"`, `content = "payload"` (codex-rs/protocol/src/protocol.rs).
const CODEX_ERROR_LAST: &str = concat!(
    r#"{"timestamp":"t0","type":"session_meta"}"#,
    "\n",
    r#"{"timestamp":"t1","type":"event_msg","payload":{"type":"error","message":"request timed out"}}"#,
    "\n",
);

/// The same rollout, then an assistant response item after the error.
fn codex_error_recovered() -> String {
    format!(
        "{CODEX_ERROR_LAST}{}\n",
        r#"{"timestamp":"t2","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"made it"}]}}"#
    )
}

/// A gate that matches when the provider's own failure says `needle`.
fn error_gate(file: &'static str, id: &str, needle: &str) -> GateFixture {
    let content: &'static str = Box::leak(
        format!(
            r#"{{
  "id": "{id}",
  "description": "The test gate for a provider error.",
  "event": "ProviderError",
  "match": {{ "all": [ {{ "type": "field_regex", "field": "facts.provider_error.message", "pattern": "{needle}" }} ] }},
  "action": {{ "type": "deny", "reason": "provider error matched" }}
}}"#
        )
        .into_boxed_str(),
    );
    GateFixture { file, content }
}

fn evaluate(env: &TestEnv, input: &str, runtime: &str) -> serde_json::Value {
    let (code, stdout, _stderr) = env.run_evaluate_stdin(input, &["--json", "--runtime", runtime]);
    assert_exit_code(code, 0);
    assert_json_ok(&stdout)
}

fn provider_error_input(transcript: Option<&std::path::Path>) -> String {
    json!({
        "hook_event_name": "ProviderError",
        "session_id": "provider-error",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "transcript_path": transcript.map(|p| p.to_string_lossy()).unwrap_or_default(),
    })
    .to_string()
}

#[test]
#[serial]
fn karen_reads_the_providers_failure_from_the_claude_transcript_tail() {
    // The API-error entry is the last assistant entry: the fact says what the
    // pipe broke on, and a gate reads it through `facts.provider_error.message`.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[error_gate("pe-claude.json", "pe-claude", "timed out")],
        &[],
    );
    let transcript = env.temp_dir.path().join("transcript.jsonl");
    std::fs::write(&transcript, CLAUDE_ERROR_LAST).expect("write transcript");
    let data = evaluate(&env, &provider_error_input(Some(&transcript)), "claude");
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_clears_the_provider_error_once_a_later_turn_succeeds() {
    // A successful assistant entry after the error means the pipe works again:
    // the fact is absent, and the gate stays quiet. A stale apology is not a fact.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[error_gate("pe-recovered.json", "pe-recovered", "timed out")],
        &[],
    );
    let transcript = env.temp_dir.path().join("transcript.jsonl");
    std::fs::write(&transcript, claude_error_recovered()).expect("write transcript");
    let data = evaluate(&env, &provider_error_input(Some(&transcript)), "claude");
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_reads_the_providers_failure_from_the_codex_rollout() {
    // Codex's rollout names the error in an `event_msg`/`error` payload
    // (shape verified against the codex-rs source); she reads the last one.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[error_gate("pe-codex.json", "pe-codex", "timed out")],
        &[],
    );
    let rollout = env.temp_dir.path().join("rollout.jsonl");
    std::fs::write(&rollout, CODEX_ERROR_LAST).expect("write rollout");
    let data = evaluate(&env, &provider_error_input(Some(&rollout)), "codex");
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_clears_the_codex_provider_error_once_a_response_lands_after_it() {
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[error_gate(
            "pe-codex-recovered.json",
            "pe-codex-recovered",
            "timed out",
        )],
        &[],
    );
    let rollout = env.temp_dir.path().join("rollout.jsonl");
    std::fs::write(&rollout, codex_error_recovered()).expect("write rollout");
    let data = evaluate(&env, &provider_error_input(Some(&rollout)), "codex");
    assert_evaluate_action(&data, "allow");
}

#[test]
#[serial]
fn karen_honors_a_runtime_supplied_provider_error_over_her_own_reading() {
    // The bridge supplies the fact: she reads nothing of her own.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[error_gate("pe-supplied.json", "pe-supplied", "overloaded")],
        &[],
    );
    let input = json!({
        "hook_event_name": "ProviderError",
        "session_id": "pe-supplied",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "facts": { "runtime": "opencode", "provider_error": { "message": "provider overloaded, try smaller steps" } },
    })
    .to_string();
    let data = evaluate(&env, &input, "opencode");
    assert_evaluate_action(&data, "deny");
}

#[test]
#[serial]
fn karen_leaves_the_provider_error_absent_without_a_transcript() {
    // Nothing to read: the fact is absent, and a gate on it stays quiet —
    // absence is not a value, and never a match.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[error_gate("pe-absent.json", "pe-absent", "timed out")],
        &[],
    );
    let data = evaluate(&env, &provider_error_input(None), "claude");
    assert_evaluate_action(&data, "allow");
}
