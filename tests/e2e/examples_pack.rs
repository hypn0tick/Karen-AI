// tests/e2e/examples_pack.rs
// The Karen who tests the starter pack as it ships — copied from the
// repo's real `examples/` directory, not a parallel universe that can
// drift. If an example rots, this file is where it gets caught, named,
// and denied — examples that lie are vaporware with a directory.

use crate::e2e::common::assertions::{
    assert_hook_allow, assert_hook_deny, assert_hook_deny_once_first, assert_hook_modify,
    assert_hook_warned_retry, assert_json_ok,
};
use crate::e2e::common::{TestEnv, karen_binary_path};
use std::path::Path;

/// Build an isolated environment from the SHIPPED examples: every gate
/// in `examples/gates/`, the annotated `config.toml`, and the example
/// rule. The Karen who grades the actual curriculum, not the syllabus.
fn examples_env() -> TestEnv {
    let temp_dir = tempfile::TempDir::new().expect("temp dir");
    let examples_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let policies_dir = temp_dir.path().join("policies");
    let home_dir = temp_dir.path().join("home");

    std::fs::create_dir_all(policies_dir.join("gates")).expect("gates dir");
    std::fs::create_dir_all(policies_dir.join("rules")).expect("rules dir");
    std::fs::create_dir_all(&home_dir).expect("home dir");

    let config =
        std::fs::read_to_string(examples_root.join("config.toml")).expect("examples config");
    std::fs::write(policies_dir.join("config.toml"), config).expect("write config");

    let mut copied = 0;
    for entry in std::fs::read_dir(examples_root.join("gates"))
        .expect("examples gates dir")
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            std::fs::copy(&path, policies_dir.join("gates").join(entry.file_name()))
                .unwrap_or_else(|e| panic!("copy example gate {}: {e}", path.display()));
            copied += 1;
        }
    }
    // The pack ships fifteen files — twelve fundamentals (one holding
    // two gates) and three advanced merges (holding two, two, and
    // three). If this number changes, she wants to know about it here
    // first — she counts, then counts again.
    assert_eq!(copied, 15, "expected all fifteen example gate files");

    std::fs::copy(
        examples_root.join("rule-00-briefing.md"),
        policies_dir.join("rules").join("rule-00-briefing.md"),
    )
    .expect("copy example rule");

    TestEnv {
        temp_dir,
        policies_dir,
        home_dir,
        karen_binary: karen_binary_path(),
    }
}

/// Build a hook payload with structural escaping — commands full of
/// quotes are exactly how string-built JSON lies to you. The Karen who
/// learned the hard way (one test at a time) that hand-rolled JSON is
/// a confession of intent, not a payload.
fn hook_input(
    event: &str,
    session: &str,
    tool: Option<&str>,
    tool_input: serde_json::Value,
    prompt: Option<&str>,
) -> String {
    let mut payload = serde_json::json!({
        "hook_event_name": event,
        "session_id": session,
    });
    if let Some(tool) = tool {
        payload["tool_name"] = serde_json::json!(tool);
        payload["tool_input"] = tool_input;
    }
    if let Some(prompt) = prompt {
        payload["prompt"] = serde_json::json!(prompt);
    }
    payload.to_string()
}

fn pretool(session: &str, command: &str) -> String {
    hook_input(
        "PreToolUse",
        session,
        Some("Bash"),
        serde_json::json!({ "command": command }),
        None,
    )
}

fn pretool_tool(session: &str, tool: &str, input: &str) -> String {
    let tool_input: serde_json::Value = serde_json::from_str(input).expect("valid tool_input JSON");
    hook_input("PreToolUse", session, Some(tool), tool_input, None)
}

fn user_prompt(session: &str, prompt: &str) -> String {
    hook_input(
        "UserPromptSubmit",
        session,
        None,
        serde_json::json!({}),
        Some(prompt),
    )
}

fn post_tool_write(session: &str, content: &str) -> String {
    hook_input(
        "PostToolUse",
        session,
        Some("Write"),
        serde_json::json!({ "content": content }),
        None,
    )
}

#[test]
fn karen_examples_pack_validates_exactly_as_shipped() {
    // The whole pack, compiled and counted: twenty gates across fifteen
    // files, one delivered rule. She grades the curriculum before she
    // teaches it. When the pack grows, update the count here — the
    // count is a feature, not a chore.
    let env = examples_env();
    let (code, stdout, _) = env.run_karen(&["validate", "--json"]);
    assert_eq!(code, 0, "the pack as shipped must validate clean");
    let data = assert_json_ok(&stdout);
    assert_eq!(
        data["gate_count"].as_u64(),
        Some(20),
        "the pack ships exactly twenty gates — she counted twice"
    );
    assert_eq!(
        data["rule_count"].as_u64(),
        Some(1),
        "the pack ships exactly one example rule"
    );
}

#[test]
fn karen_examples_force_push_denied() {
    // The classic: rewriting shared history gets a citation, not a pass.
    let env = examples_env();
    let (code, stdout, _) =
        env.run_hook(&pretool("ex-force", r"git push --force origin main"), &[]);
    assert_eq!(code, 0);
    assert_hook_deny(&stdout, Some("shared history"));
}

#[test]
fn karen_examples_jq_teaches_exactly_once() {
    // The canonical teaching gate: strike one denies with the fix;
    // the retry passes with her closing remarks. The lesson is finite;
    // the memory is not.
    let env = examples_env();
    let (code, first, _) = env.run_hook(&pretool("ex-jq", r"jq '.x' config.yaml"), &[]);
    assert_eq!(code, 0);
    assert_hook_deny_once_first(&first, Some("yq"));
    let (code, second, _) = env.run_hook(&pretool("ex-jq", r"jq '.x' config.yaml"), &[]);
    assert_eq!(code, 0);
    assert_hook_warned_retry(&second, Some("saw the whole thing"));
}

#[test]
fn karen_examples_head_rewrites_in_flight() {
    // The engine's one shipped modifier, live: head becomes the
    // portable equivalent before the command ever runs.
    let env = examples_env();
    let (code, stdout, _) = env.run_hook(&pretool("ex-head", r"head -n 5 readme.txt"), &[]);
    assert_eq!(code, 0);
    assert_hook_modify(&stdout, Some("Select-Object"));
}

#[test]
fn karen_examples_secrets_never_ride_in_commands() {
    // A GitHub token pasted into a command: denied before the ink
    // dries, with the rotation lecture attached.
    let env = examples_env();
    let (code, stdout, _) = env.run_hook(
        &pretool(
            "ex-secret",
            r#"curl -H "Authorization: token ghp_abcdefghijklmnopqrstuvwxyz0123456789" https://api.example.com"#,
        ),
        &[],
    );
    assert_eq!(code, 0);
    assert_hook_deny(&stdout, Some("rotate"));
}

#[test]
fn karen_examples_lockfile_edits_rejected() {
    // Lockfiles are generated state. Use the package manager; she
    // uses the policy.
    let env = examples_env();
    let (code, stdout, _) = env.run_hook(
        &pretool_tool(
            "ex-lock",
            "Edit",
            r#"{"file_path":"package-lock.json","new_string":"resolved by hand"}"#,
        ),
        &[],
    );
    assert_eq!(code, 0);
    assert_hook_deny(&stdout, Some("package manager"));
}

#[test]
fn karen_examples_rehearsal_passes_performance_escalates() {
    // The precedence law, live: --dry-run deletes sail through (the
    // exclusion lives in the ask's own `none` block — an allow never
    // overrules anything); the live delete goes to the manager.
    let env = examples_env();
    let (code, rehearsal, _) = env.run_hook(
        &pretool(
            "ex-rehearsal",
            r"kubectl delete pods legacy --dry-run=client",
        ),
        &[],
    );
    assert_eq!(code, 0);
    assert_hook_allow(&rehearsal);

    let (code, performance, _) =
        env.run_hook(&pretool("ex-rehearsal", r"kubectl delete pods legacy"), &[]);
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(&performance).expect("valid JSON");
    assert_eq!(
        value["hookSpecificOutput"]["permissionDecision"], "ask",
        "live deletes must escalate to the manager: {performance}"
    );
}

#[test]
fn karen_examples_scope_creep_named_once() {
    // "While you're in there" is not a scope decision: named once,
    // then noted forever.
    let env = examples_env();
    let (code, first, _) = env.run_hook(
        &user_prompt("ex-scope", r"While you're in there, also update the docs."),
        &[],
    );
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(&first).expect("valid JSON");
    assert_eq!(
        value["decision"], "block",
        "the first scope creep must block the prompt with a reason: {first}"
    );
    let (code, second, _) = env.run_hook(
        &user_prompt("ex-scope", r"While you're in there, also update the docs."),
        &[],
    );
    assert_eq!(code, 0);
    assert_hook_warned_retry(&second, Some("noted"));
}

#[test]
fn karen_examples_stale_fact_prompts_returned_to_sender() {
    // "Latest version" in a prompt: blocked with instructions to go
    // check. Today's facts have a shelf life; she checks the dates.
    let env = examples_env();
    let (code, stdout, _) = env.run_hook(
        &user_prompt("ex-fresh", r"What's the latest version of the framework?"),
        &[],
    );
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        value["decision"], "block",
        "current-fact assertions must be blocked pending a search: {stdout}"
    );
}

#[test]
fn karen_examples_evidence_pipeline_denies_then_acknowledges() {
    // The advanced pipeline: uncited claims get one hard look; cited
    // claims get an acknowledgment EVERY time — the max_strikes: 1
    // discovery means the acknowledgment gate never reaches its denial
    // branch. Receipts, filed, twice.
    let env = examples_env();
    let (code, uncited, _) = env.run_hook(
        &post_tool_write("ex-evidence", r"Studies show this works."),
        &[],
    );
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(&uncited).expect("valid JSON");
    assert_eq!(
        value["decision"], "block",
        "uncited claims must block: {uncited}"
    );

    for _ in 0..2 {
        let (code, cited, _) = env.run_hook(
            &post_tool_write(
                "ex-evidence-ack",
                r"Studies show this works (source: https://example.com/study).",
            ),
            &[],
        );
        assert_eq!(code, 0);
        assert_hook_warned_retry(&cited, Some("receipts accepted"));
    }
}

#[test]
fn karen_examples_prod_namespace_escalates_by_exact_field() {
    // field_equals on structured payloads: she reads the field, not
    // the vibes. namespace IS prod → the desk, with the file open.
    let env = examples_env();
    let (code, stdout, _) = env.run_hook(
        &pretool_tool(
            "ex-prod",
            "Helm",
            r#"{"namespace":"prod","release":"billing"}"#,
        ),
        &[],
    );
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        value["hookSpecificOutput"]["permissionDecision"], "ask",
        "prod-namespace calls must escalate: {stdout}"
    );
}

#[test]
fn karen_examples_ci_workflow_edits_get_a_human() {
    // Pipeline surgery goes to the one person authorized for it.
    let env = examples_env();
    let (code, stdout, _) = env.run_hook(
        &pretool_tool(
            "ex-ci",
            "Edit",
            r#"{"file_path":".github/workflows/deploy.yml","new_string":"steps: []"}"#,
        ),
        &[],
    );
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        value["hookSpecificOutput"]["permissionDecision"], "ask",
        "CI workflow edits must escalate: {stdout}"
    );
}

#[test]
fn karen_examples_freeze_ships_off() {
    // The emergency brake, verified OFF: benign commands pass because
    // the always-gate is disabled — a brake you didn't apply is not a
    // brake, it's a trap.
    let env = examples_env();
    let (code, stdout, _) = env.run_hook(&pretool("ex-freeze", r"echo deploy production"), &[]);
    assert_eq!(code, 0);
    assert_hook_allow(&stdout);
}

#[test]
fn karen_examples_briefing_delivered_at_session_start() {
    // The delivered-prose side of the house: the example rule arrives
    // as part 1 at session start, and there is no part 2 — the
    // briefing fits in one sitting, which is the point of briefings.
    let env = examples_env();
    let input =
        r#"{"hook_event_name":"SessionStart","session_id":"ex-briefing","source":"startup"}"#;
    let (code, part1, _) = env.run_hook(input, &["--part", "1"]);
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(&part1).expect("valid JSON");
    let ctx = value["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_else(|| panic!("part 1 must deliver the briefing: {part1}"));
    assert!(
        ctx.contains("Standing Briefing"),
        "the briefing must arrive, by name: {part1}"
    );
    let (code, part2, _) = env.run_hook(input, &["--part", "2"]);
    assert_eq!(code, 0);
    assert_hook_allow(&part2);
}
