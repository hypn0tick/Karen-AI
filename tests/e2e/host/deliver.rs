// tests/e2e/host/deliver.rs
// The Karen who hands a hosted session a command, and waits her turn to type it.
//
// The hook side is the real `karen hook` with the `KAREN_HOST_*` variables a hosted runtime's hooks
// inherit; the host side is the real `karen run` on the fake runtime. Nothing types into an agent.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use karen::host::endpoint::{self, DeliveryMeta, Handler, Request, Response};
use serde_json::{Value, json};
use serial_test::serial;

use super::{Hosted, eventually};
use crate::e2e::common::commands::{command_gate, runtime_lines, user_layer};
use crate::e2e::common::{DEFAULT_CONFIG, GateFixture, TestEnv, assert_exit_code};

const CONFIG: &str = "[actions]\nruntime_commands = [\"/compact\"]\n\n[host]\nquiet_ms = 100\n";

/// A user layer allowing `/compact`, with a gate that types it on every tool call, its arguments
/// naming the session.
fn env_with_gate(extra_config: &str) -> TestEnv {
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let gate = command_gate(
        "compact-it",
        "PreToolUse",
        "/compact",
        Some("for {facts.session_id}"),
        true,
    );
    user_layer(
        &env,
        &format!("{CONFIG}{extra_config}"),
        &[("compact.json", gate)],
    );
    env
}

fn tool_call(session: &str) -> String {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": session,
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "facts": {},
    })
    .to_string()
}

/// The variables a hosted runtime's hook process inherits from the host.
fn inherited(host: &Hosted) -> Vec<(String, String)> {
    ["ENDPOINT", "TOKEN", "RUNTIME", "PID"]
        .iter()
        .map(|name| {
            let key = format!("KAREN_HOST_{name}");
            let value = host.env(&key);
            (key, value)
        })
        .collect()
}

/// Run one `karen hook --runtime claude` call the way a hosted Claude Code would: with the host's
/// variables in its environment.
fn hook_under(env: &TestEnv, host: &Hosted, session: &str) -> Value {
    let pairs = inherited(host);
    let pairs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let (code, stdout, _stderr) =
        env.run_hook_with_env(&tool_call(session), &["--runtime", "claude"], &pairs);
    assert_exit_code(code, 0);
    serde_json::from_str(stdout.trim()).expect("a hook answers in JSON")
}

fn wait_for_status(env: &TestEnv, session: &str, status: &str) -> Value {
    eventually(&format!("{session} to be filed as {status}"), || {
        runtime_lines(env)
            .into_iter()
            .find(|line| line["session"] == session && line["status"] == status)
    })
}

fn prompt_is_up(host: &Hosted) {
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));
}

#[test]
#[serial(host_delivery)]
fn karen_delivers_a_command_to_a_hosted_session_once_it_is_idle() {
    let env = env_with_gate("");
    let host = Hosted::start_in(&env.home_dir, Some("claude"), &[], 100);
    host.await_log("started");
    prompt_is_up(&host);

    let answer = hook_under(&env, &host, "dl-1");

    assert!(
        answer.get("decision").is_none() && answer.get("hookSpecificOutput").is_none(),
        "a typed command is a side effect, not a verdict: {answer}"
    );
    let handed = wait_for_status(&env, "dl-1", "handed-off");
    assert_eq!(handed["layer"], "terminal-host");
    assert_eq!(
        handed["arguments"], "for {facts.session_id}",
        "the template"
    );
    let sent = wait_for_status(&env, "dl-1", "sent");
    assert_eq!(sent["layer"], "terminal-host");
    assert_eq!(sent["route"], "karen run");
    assert_eq!(sent["command"], "/compact");
    assert_eq!(sent["arguments"], "for {facts.session_id}");
    host.await_log("line \"/compact for dl-1\"");
    host.screen_until("the runtime's reply", |text| {
        text.contains("you said: /compact for dl-1")
    });
    assert!(
        !std::fs::read_to_string(env.home_dir.join(".karen/logs/karen-runtime.jsonl"))
            .unwrap()
            .contains("for dl-1"),
        "the rendered line never reaches the ledger"
    );
}

#[test]
#[serial(host_delivery)]
fn karen_queues_commands_for_a_busy_session_and_types_them_in_order() {
    let env = env_with_gate("");
    let mut host = Hosted::start_in(&env.home_dir, Some("claude"), &[], 1500);
    host.await_log("started");
    prompt_is_up(&host);
    host.type_into_stdin(b"start a long turn\r");
    host.await_log("line \"start a long turn\"");

    hook_under(&env, &host, "dl-a");
    hook_under(&env, &host, "dl-b");

    let first = wait_for_status(&env, "dl-a", "sent");
    let second = wait_for_status(&env, "dl-b", "sent");
    let log = host.await_log("line \"/compact for dl-b\"");
    let lines: Vec<&str> = log.lines().filter(|l| l.starts_with("line ")).collect();
    assert_eq!(
        lines,
        [
            "line \"start a long turn\"",
            "line \"/compact for dl-a\"",
            "line \"/compact for dl-b\""
        ],
        "first in, first typed, and not into the turn in flight"
    );
    assert!(
        first["waited_ms"].as_u64().unwrap() >= 1000,
        "it waited out the busy turn: {first}"
    );
    assert!(
        second["time"].as_u64().unwrap() >= first["time"].as_u64().unwrap() + 1500,
        "one command per idle, the next waits for the turn the first started: {first} {second}"
    );
}

#[test]
#[serial(host_delivery)]
fn karen_never_types_into_a_half_typed_prompt_and_files_the_timeout() {
    let env = env_with_gate("timeout_secs = 3\n");
    let mut host = Hosted::start_in(&env.home_dir, Some("claude"), &[], 100);
    host.await_log("started");
    prompt_is_up(&host);
    host.type_into_stdin(b"half a thou");
    host.screen_until("the half-typed prompt", |text| {
        text.contains("> half a thou")
    });

    hook_under(&env, &host, "dl-half");

    let filed = wait_for_status(&env, "dl-half", "undelivered:timeout");
    assert_eq!(filed["layer"], "terminal-host");
    assert!(filed["waited_ms"].as_u64().unwrap() >= 3000, "{filed}");
    assert!(
        !host.log().contains("/compact"),
        "the owner's half-typed prompt stayed theirs: {}",
        host.log()
    );
}

#[test]
#[serial(host_delivery)]
fn karen_files_what_is_still_waiting_when_the_runtime_exits() {
    let env = env_with_gate("");
    let mut host = Hosted::start_in(&env.home_dir, Some("claude"), &[], 3000);
    host.await_log("started");
    prompt_is_up(&host);
    host.type_into_stdin(b"a turn that outlives the queue\r");
    host.await_log("line \"a turn that outlives the queue\"");
    hook_under(&env, &host, "dl-last");
    host.type_into_stdin(b"/exit\r");

    wait_for_status(&env, "dl-last", "undelivered:host-exited");
    assert_eq!(host.exit_code(), 0);
    assert!(!host.log().contains("/compact"));
}

#[test]
#[serial(host_delivery)]
fn karen_does_not_deliver_to_a_host_running_another_runtime() {
    let env = env_with_gate("");
    let host = Hosted::start_in(&env.home_dir, Some("codex"), &[], 100);
    host.await_log("started");
    prompt_is_up(&host);

    hook_under(&env, &host, "dl-wrong");

    let line = wait_for_status(&env, "dl-wrong", "undelivered:no-layer");
    assert!(line["layer"].is_null(), "{line}");
    std::thread::sleep(Duration::from_millis(500));
    assert!(!host.log().contains("/compact"), "{}", host.log());
}

#[test]
#[serial(host_delivery)]
fn karen_honours_the_hosts_configured_timings() {
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    user_layer(
        &env,
        "[host]\ntimeout_secs = 0\nquiet_ms = 5\nenter_delay_ms = 99999\n",
        &[],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let issues = stdout;
    for key in ["timeout_secs = 0", "quiet_ms = 5", "enter_delay_ms = 99999"] {
        assert!(
            issues.contains(&format!("ignores [host] {key}")),
            "validate names `{key}`: {issues}"
        );
    }
}

#[test]
#[serial(host_delivery)]
fn karen_ignores_a_project_layers_host_settings_and_validate_says_so() {
    let env = TestEnv::with_config(
        "[host]
quiet_ms = 9000
",
        &[],
        &[],
    );
    user_layer(&env, CONFIG, &[]);
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    assert!(
        stdout.contains("sets [host]") && stdout.contains("only the user layer's is read"),
        "{stdout}"
    );
}

/// An endpoint in the test process that records what it is asked, standing in for a host.
struct Recorder {
    requests: Mutex<Vec<Request>>,
    answer: Response,
}

impl Handler for Recorder {
    fn handle(&self, request: &Request) -> Response {
        self.requests.lock().unwrap().push(request.clone());
        self.answer.clone()
    }
}

/// A recording endpoint and the variables that point a hook at it.
fn recording(
    runtime: &str,
    answer: Response,
) -> (Arc<Recorder>, Vec<(String, String)>, endpoint::Endpoint) {
    let (listener, endpoint) = endpoint::bind().expect("an endpoint");
    let recorder = Arc::new(Recorder {
        requests: Mutex::new(Vec::new()),
        answer,
    });
    endpoint::serve(listener, endpoint.token.clone(), recorder.clone());
    let vars = vec![
        ("KAREN_HOST_ENDPOINT".to_string(), endpoint.name.clone()),
        ("KAREN_HOST_TOKEN".to_string(), endpoint.token.clone()),
        ("KAREN_HOST_RUNTIME".to_string(), runtime.to_string()),
    ];
    (recorder, vars, endpoint)
}

fn hook_with(env: &TestEnv, vars: &[(String, String)], args: &[&str]) -> Value {
    let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (code, stdout, _stderr) = env.run_hook_with_env(&tool_call("rec-1"), args, &pairs);
    assert_exit_code(code, 0);
    serde_json::from_str(stdout.trim()).expect("JSON")
}

#[test]
#[serial(host_delivery)]
fn karen_hands_the_host_the_rendered_line_and_the_unrendered_template() {
    let env = env_with_gate("");
    let (recorder, vars, endpoint) = recording("claude", Response::ok());

    hook_with(&env, &vars, &["--runtime", "claude"]);

    let seen = recorder.requests.lock().unwrap().clone();
    assert_eq!(
        seen,
        [Request::Deliver {
            text: "/compact for rec-1".into(),
            meta: DeliveryMeta {
                session: "rec-1".into(),
                gate: "compact-it".into(),
                runtime: "claude".into(),
                command: "/compact".into(),
                arguments: Some("for {facts.session_id}".into()),
            },
        }],
        "the host is told the line, and the template for its ledger"
    );
    let lines = runtime_lines(&env);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["status"], "handed-off");
    assert_eq!(lines[0]["layer"], "terminal-host");
    endpoint.close();
}

#[test]
#[serial(host_delivery)]
fn karen_never_opens_the_endpoint_for_an_attempt_the_owner_did_not_allow() {
    // The allowlist lives on the hook side. A host that is asked is a host that may type, so every
    // refusal here must happen before a single byte is sent to it.
    let (recorder, vars, endpoint) = recording("claude", Response::ok());

    // Off the allowlist.
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let off_list = command_gate("off-list", "PreToolUse", "/clear", None, true);
    user_layer(&env, CONFIG, &[("clear.json", off_list)]);
    hook_with(&env, &vars, &["--runtime", "claude"]);
    assert_eq!(runtime_lines(&env)[0]["status"], "refused:not-allowlisted");

    // A project layer's gate.
    let project = command_gate("project-gate", "PreToolUse", "/compact", None, true);
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[GateFixture {
            file: "p.json",
            content: Box::leak(project.into_boxed_str()),
        }],
        &[],
    );
    user_layer(&env, CONFIG, &[]);
    hook_with(&env, &vars, &["--runtime", "claude"]);
    assert_eq!(runtime_lines(&env)[0]["status"], "refused:not-user-layer");

    // Text that cannot be typed.
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    let unsafe_text = command_gate(
        "unsafe-text",
        "PreToolUse",
        "/compact",
        Some("two\nlines"),
        true,
    );
    user_layer(&env, CONFIG, &[("u.json", unsafe_text)]);
    hook_with(&env, &vars, &["--runtime", "claude"]);
    assert_eq!(runtime_lines(&env)[0]["status"], "undelivered:unsafe-text");

    // A rehearsal files nothing and sends nothing.
    let env = env_with_gate("");
    let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (code, _stdout, _stderr) = env.run_evaluate_stdin_with_env(
        &tool_call("rec-2"),
        &["--json", "--runtime", "claude"],
        &pairs,
    );
    assert_exit_code(code, 0);
    assert_eq!(runtime_lines(&env), Vec::<Value>::new());

    assert!(
        recorder.requests.lock().unwrap().is_empty(),
        "no refused attempt reached the endpoint: {:?}",
        recorder.requests.lock().unwrap()
    );
    endpoint.close();
}

#[test]
#[serial(host_delivery)]
fn karen_files_a_host_that_refuses_and_a_host_that_is_gone() {
    let env = env_with_gate("");

    let (_recorder, vars, endpoint) = recording("claude", Response::refused("the queue is full"));
    hook_with(&env, &vars, &["--runtime", "claude"]);
    assert_eq!(runtime_lines(&env)[0]["status"], "undelivered:host-refused");
    endpoint.close();

    let env = env_with_gate("");
    let gone = vec![
        (
            "KAREN_HOST_ENDPOINT".to_string(),
            "karen-host-gone".to_string(),
        ),
        ("KAREN_HOST_TOKEN".to_string(), "t".to_string()),
        ("KAREN_HOST_RUNTIME".to_string(), "claude".to_string()),
    ];
    hook_with(&env, &gone, &["--runtime", "claude"]);
    let line = runtime_lines(&env)[0].clone();
    assert_eq!(line["status"], "undelivered:host-unreachable");
    assert_eq!(line["layer"], "terminal-host", "the layer it tried: {line}");
}

#[test]
#[serial(host_delivery)]
fn karen_rehearsal_reports_the_hand_off_it_would_make() {
    let env = env_with_gate("");
    let (recorder, vars, endpoint) = recording("claude", Response::ok());
    let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let (code, stdout, _stderr) = env.run_evaluate_stdin_with_env(
        &tool_call("rec-3"),
        &["--json", "--runtime", "claude"],
        &pairs,
    );

    assert_exit_code(code, 0);
    let data: Value = serde_json::from_str(stdout.trim()).expect("JSON");
    assert_eq!(data["data"]["runtime_commands"][0]["status"], "handed-off");
    assert_eq!(
        data["data"]["runtime_commands"][0]["layer"],
        "terminal-host"
    );
    assert!(recorder.requests.lock().unwrap().is_empty());
    assert!(
        !stdout.contains("for rec-3"),
        "a rehearsal never prints the line a layer would type"
    );
    endpoint.close();
}
