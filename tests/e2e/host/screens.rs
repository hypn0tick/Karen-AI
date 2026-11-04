// tests/e2e/host/screens.rs
// The Karen who reads the sign on the door: `facts.screen.*` off a session hosted by `karen run`.
//
// The host side is the real `karen run` on the fake runtime; the hook side is the real `karen hook`
// with the variables a hosted runtime's hooks inherit. The screens are drawn by the fake runtime
// and read through the host's endpoint, and three of them are kept in `tests/fixtures/screens/` as
// the recorded screens the extractors' unit tests read. Nothing here types into an agent.

use std::path::{Path, PathBuf};

use karen::host::Request;
use serde_json::{Value, json};
use serial_test::serial;

use super::Hosted;
use crate::e2e::common::commands::user_layer;
use crate::e2e::common::{DEFAULT_CONFIG, TestEnv, assert_exit_code};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens")
        .join(name)
}

/// A screen as it is kept: no trailing blanks on a row, none after the last row, so a repaint that
/// pads differently is the same screen.
fn kept(text: &str) -> String {
    let rows: Vec<&str> = text.lines().map(str::trim_end).collect();
    let used = rows
        .iter()
        .rposition(|row| !row.is_empty())
        .map_or(0, |last| last + 1);
    format!("{}\n", rows[..used].join("\n"))
}

/// A user-layer gate that denies a tool call when `condition` holds.
fn deny_when(id: &str, condition: &Value, reason: &str) -> (String, String) {
    let gate = json!({
        "id": id,
        "description": "The test gate for a screen fact.",
        "event": "PreToolUse",
        "match": { "all": [condition] },
        "action": { "type": "deny", "reason": reason },
    });
    (format!("{id}.json"), gate.to_string())
}

fn env_with(gates: &[(String, String)]) -> TestEnv {
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let files: Vec<(&str, String)> = gates
        .iter()
        .map(|(file, content)| (file.as_str(), content.clone()))
        .collect();
    user_layer(&env, "", &files);
    env
}

fn tool_call(extra: &Value) -> String {
    let mut payload = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "screens-1",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
    });
    if let (Some(into), Some(from)) = (payload.as_object_mut(), extra.as_object()) {
        into.extend(from.clone());
    }
    payload.to_string()
}

/// One `karen hook --runtime claude` call, under `host` when given, as the answer it printed.
fn hook(env: &TestEnv, host: Option<&Hosted>, extra: &Value) -> Value {
    let pairs: Vec<(String, String)> = host.map_or_else(Vec::new, |host| {
        ["ENDPOINT", "TOKEN", "RUNTIME", "PID"]
            .iter()
            .map(|name| {
                let key = format!("KAREN_HOST_{name}");
                let value = host.env(&key);
                (key, value)
            })
            .collect()
    });
    let pairs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let (code, stdout, _stderr) =
        env.run_hook_with_env(&tool_call(extra), &["--runtime", "claude"], &pairs);
    assert_exit_code(code, 0);
    serde_json::from_str(stdout.trim()).expect("a hook answers in JSON")
}

fn denied(answer: &Value) -> bool {
    answer["hookSpecificOutput"]["permissionDecision"] == "deny"
}

fn prompt_is_up(host: &Hosted) -> String {
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'))
}

/// Type `line` into the hosted runtime now and wait for its prompt to come back below `marker`.
fn say(host: &Hosted, line: &str, marker: &str) -> String {
    let answer = host.call(&Request::Type {
        text: line.to_string(),
    });
    assert!(answer.ok, "the host types {line}: {answer:?}");
    host.screen_until(marker, |text| {
        text.contains(marker) && text.trim_end().ends_with('>')
    })
}

#[test]
#[serial(host_delivery)]
fn the_screens_the_fake_runtime_draws_are_the_recorded_fixtures() {
    // The fixtures are screens recorded from a hosted session. `KAREN_RECORD_SCREENS=1` records them
    // again; without it a screen the host reads differently fails here.
    let record = std::env::var_os("KAREN_RECORD_SCREENS").is_some();
    let host = Hosted::start(&[], 50);
    host.await_log("started");
    let mut seen = vec![("fake-runtime-ready.txt", prompt_is_up(&host))];
    seen.push(("fake-runtime-gauge.txt", say(&host, "/gauge", "Context")));
    seen.push((
        "fake-runtime-limit.txt",
        say(&host, "/limit", "usage limit"),
    ));
    for (name, text) in seen {
        let text = kept(&text);
        if record {
            std::fs::create_dir_all(fixture(name).parent().unwrap()).unwrap();
            std::fs::write(fixture(name), &text).unwrap();
        } else {
            let recorded = std::fs::read_to_string(fixture(name)).expect("a recorded screen");
            assert_eq!(kept(&recorded), text, "{name}");
        }
    }
}

#[test]
#[serial(host_delivery)]
fn a_gate_reads_the_screen_of_the_session_it_runs_under() {
    let at =
        |field: &str, value: Value| json!({"type": "field_equals", "field": field, "value": value});
    let gates = [
        deny_when(
            "prompt-empty",
            &at("facts.screen.prompt_empty", json!(true)),
            "empty",
        ),
        deny_when(
            "turn-running",
            &at("facts.screen.turn_running", json!(true)),
            "running",
        ),
        deny_when(
            "limit",
            &at("facts.screen.usage_limit", json!(true)),
            "limit",
        ),
        deny_when(
            "gauge",
            &json!({"type": "field_gte", "field": "facts.screen.context_used_percent", "value": 50}),
            "gauge",
        ),
        deny_when(
            "says-ready",
            &json!({"type": "field_regex", "field": "facts.screen.text", "pattern": "fake-runtime ready"}),
            "text",
        ),
    ];
    // A host for another runtime is not this session's keyboard, nor its screen.
    let other_home = tempfile::tempdir().unwrap();
    let other = Hosted::start_in(other_home.path(), Some("codex"), &[], 50);
    other.await_log("started");
    prompt_is_up(&other);
    let all = env_with(&gates);
    assert!(
        !denied(&hook(&all, Some(&other), &json!({}))),
        "a host for another runtime is not this session's screen"
    );
    drop(other);

    let home = tempfile::tempdir().unwrap();
    let host = Hosted::start_in(home.path(), Some("claude"), &[], 50);
    host.await_log("started");
    prompt_is_up(&host);
    assert!(
        denied(&hook(&all, Some(&host), &json!({}))),
        "the same gates fire under the host of this runtime"
    );

    // Each gate alone, so one that fires cannot hide one that should.
    let expect = |gate: &str, fires: bool, when: &str, host: &Hosted| {
        let (file, content) = gates
            .iter()
            .find(|(file, _)| file.starts_with(gate))
            .expect("a gate");
        let env = env_with(&[(file.clone(), content.clone())]);
        let answer = hook(&env, Some(host), &json!({}));
        assert_eq!(denied(&answer), fires, "{gate} {when}: {answer}");
    };
    expect("prompt-empty", true, "at the ready prompt", &host);
    expect("turn-running", false, "at the ready prompt", &host);
    expect("limit", false, "at the ready prompt", &host);
    expect("gauge", false, "at the ready prompt", &host);
    expect("says-ready", true, "at the ready prompt", &host);

    say(&host, "/gauge", "Context");
    expect("gauge", true, "with a 67% gauge", &host);
    expect("limit", false, "with a 67% gauge", &host);

    say(&host, "/limit", "usage limit");
    expect("limit", true, "with a banner", &host);
}

#[test]
#[serial(host_delivery)]
fn a_session_with_no_host_has_no_screen_facts_whatever_the_payload_says() {
    let condition =
        json!({"type": "field_equals", "field": "facts.screen.usage_limit", "value": true});
    let env = env_with(&[deny_when("limit", &condition, "limit")]);
    let spoofed = json!({"facts": {"screen": {"usage_limit": true, "text": "x"}}});
    assert!(
        !denied(&hook(&env, None, &spoofed)),
        "only the terminal host can say what is on a screen"
    );
}

#[test]
#[serial(host_delivery)]
fn screen_text_is_matched_but_never_rendered_or_written_down() {
    let condition = json!({"type": "field_regex", "field": "facts.screen.text", "pattern": "fake-runtime ready"});
    let env = env_with(&[deny_when(
        "says-ready",
        &condition,
        "the screen says {facts.screen.text}",
    )]);
    let home = tempfile::tempdir().unwrap();
    let host = Hosted::start_in(home.path(), Some("claude"), &[], 50);
    host.await_log("started");
    prompt_is_up(&host);
    say(&host, "/limit", "usage limit");

    let answer = hook(&env, Some(&host), &json!({}));

    assert!(denied(&answer), "{answer}");
    let reason = answer["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap();
    assert!(
        reason.contains("{facts.screen.text}"),
        "the placeholder stays literal: {reason}"
    );
    assert!(!reason.contains("usage limit"), "{reason}");
    for file in walk(&env.home_dir) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        assert!(
            !text.contains("Your limit will reset"),
            "{} holds screen text",
            file.display()
        );
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path);
            }
        }
    }
    found
}

#[test]
#[serial]
fn karen_validate_names_a_screen_fact_templated_into_a_reason() {
    let condition =
        json!({"type": "field_equals", "field": "facts.screen.usage_limit", "value": true});
    let env = env_with(&[deny_when("limit", &condition, "see {facts.screen.text}")]);
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data: Value = serde_json::from_str(stdout.trim()).expect("validate prints JSON");
    let messages: Vec<String> = data["data"]["issues"]
        .as_array()
        .or_else(|| data["issues"].as_array())
        .expect("issues")
        .iter()
        .filter_map(|issue| issue["message"].as_str().map(str::to_string))
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("`limit` templates {facts.screen.text}")),
        "{messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("not a fact she has")),
        "screen is a fact she has: {messages:?}"
    );
}
