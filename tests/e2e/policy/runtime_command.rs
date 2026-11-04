// tests/e2e/policy/runtime_command.rs
// The Karen who tests the front desk: who may leave a message for a session,
// which messages she will take, and what she writes in the ledger when nobody
// is there to carry one.

use crate::e2e::common::{DEFAULT_CONFIG, GateFixture, TestEnv, assert_exit_code, assert_json_ok};
use serde_json::{Value, json};
use serial_test::serial;

/// A `runtime_command` gate that matches every tool call. `enabled` is the
/// gate's own switch.
fn gate(id: &str, command: &str, arguments: Option<&str>, enabled: bool) -> String {
    let mut action = json!({ "type": "runtime_command", "command": command });
    if let Some(arguments) = arguments {
        action["arguments"] = json!(arguments);
    }
    json!({
        "id": id,
        "description": "The test gate for runtime_command.",
        "event": "PreToolUse",
        "enabled": enabled,
        "match": { "all": [ { "type": "always" } ] },
        "action": action,
    })
    .to_string()
}

/// A fixture for the project layer. Test values leak: a fixture lives as long
/// as the process that loads it.
fn project_gate(file: &'static str, content: String) -> GateFixture {
    GateFixture {
        file,
        content: Box::leak(content.into_boxed_str()),
    }
}

/// A user layer at `<home>/.karen/policies`: the only layer that may type.
fn user_layer(env: &TestEnv, config: &str, gates: &[(&str, String)]) {
    let root = env.home_dir.join(".karen").join("policies");
    std::fs::create_dir_all(root.join("gates")).expect("create user gates dir");
    std::fs::create_dir_all(root.join("rules")).expect("create user rules dir");
    std::fs::write(
        root.join("config.toml"),
        format!("# User layer.\n\n[state]\nbackend = \"memory\"\n\n{config}\n"),
    )
    .expect("write user config");
    for (file, content) in gates {
        std::fs::write(root.join("gates").join(file), content).expect("write user gate");
    }
}

/// A hook payload for a tool call, with `facts` the way a bridge would hand
/// them over.
fn pretool(session: &str, command: &str, facts: &Value) -> String {
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": session,
        "tool_name": "Bash",
        "tool_input": { "command": command },
        "facts": facts,
    })
    .to_string()
}

/// The runtime log a test home keeps, as parsed JSON lines.
fn runtime_lines(env: &TestEnv) -> Vec<Value> {
    let log = env
        .home_dir
        .join(".karen")
        .join("logs")
        .join("karen-runtime.jsonl");
    std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// An env with an empty project layer and a user layer holding `gates`.
fn user_env(config: &str, gates: &[(&str, String)]) -> TestEnv {
    let env = TestEnv::with_config(DEFAULT_CONFIG, &[], &[]);
    user_layer(&env, config, gates);
    env
}

/// The one runtime-log line a hook call wrote.
fn only_line(env: &TestEnv) -> Value {
    let lines = runtime_lines(env);
    assert_eq!(lines.len(), 1, "one attempt, one line: {lines:?}");
    lines[0].clone()
}

fn validate_issues(env: &TestEnv) -> Vec<String> {
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    data["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .filter_map(|i| i["message"].as_str().map(str::to_string))
        .collect()
}

const ALLOW_COMPACT: &str = "[actions]\nruntime_commands = [\"/compact\"]";

#[test]
#[serial]
fn karen_logs_an_allowed_command_that_no_layer_carries() {
    // Allowed, and nothing reaches the session: the call proceeds untouched and
    // the ledger says what became of the message, with no layer named.
    let env = user_env(
        ALLOW_COMPACT,
        &[(
            "compact.json",
            gate("compact-it", "/compact", Some("keep the open items"), true),
        )],
    );
    let (code, stdout, _stderr) = env.run_hook(
        &pretool("rc-1", "echo hi", &json!({})),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    let out: Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert!(
        out.get("decision").is_none() && out.get("hookSpecificOutput").is_none(),
        "a command typed into a session is a side effect, not a verdict: {out}"
    );
    // The one thing she says aloud is the merge notice: a gate that types, and
    // a runtime nothing reaches, is a configuration the owner should hear of.
    assert!(
        out["systemMessage"].as_str().is_some_and(
            |m| m.contains("has no layer that reaches claude, codex, opencode sessions")
        ),
        "the merge notice names the unreached runtimes: {out}"
    );
    let line = only_line(&env);
    assert_eq!(line["session"], "rc-1");
    assert_eq!(line["gate"], "compact-it");
    assert_eq!(line["runtime"], "claude");
    assert_eq!(line["command"], "/compact");
    assert_eq!(line["arguments"], "keep the open items");
    assert!(
        line["layer"].is_null(),
        "no layer carried it, so none is named: {line}"
    );
    assert_eq!(line["status"], "undelivered:no-layer");
    assert!(line["time"].as_u64().is_some(), "time is epoch-ms: {line}");
}

#[test]
#[serial]
fn karen_logs_the_arguments_as_the_gate_wrote_them_never_rendered() {
    // What a session's facts rendered into the arguments is data about the
    // session; the ledger keeps the template, so a fact never lands in a log.
    let env = user_env(
        ALLOW_COMPACT,
        &[(
            "compact.json",
            gate(
                "compact-it",
                "/compact",
                Some("after: {facts.provider_error.message}"),
                true,
            ),
        )],
    );
    let facts = json!({ "provider_error": { "message": "SECRET-IN-A-FACT" } });
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-2", "echo hi", &facts),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    let line = only_line(&env);
    assert_eq!(line["arguments"], "after: {facts.provider_error.message}");
    assert_eq!(line["status"], "undelivered:no-layer");
    let raw = std::fs::read_to_string(
        env.home_dir
            .join(".karen")
            .join("logs")
            .join("karen-runtime.jsonl"),
    )
    .expect("the runtime log");
    assert!(
        !raw.contains("SECRET-IN-A-FACT"),
        "a rendered fact must never reach the ledger: {raw}"
    );
}

#[test]
#[serial]
fn karen_refuses_a_command_off_the_allowlist() {
    let env = user_env(
        ALLOW_COMPACT,
        &[("clear.json", gate("clear-it", "/clear", None, true))],
    );
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-3", "echo hi", &json!({})),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    let line = only_line(&env);
    assert_eq!(line["command"], "/clear");
    assert_eq!(line["status"], "refused:not-allowlisted");
}

#[test]
#[serial]
fn karen_allows_no_command_until_the_user_layer_names_one() {
    // Absence is not a permission: an unset allowlist is the empty one.
    let env = user_env(
        "",
        &[("compact.json", gate("compact-it", "/compact", None, true))],
    );
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-4", "echo hi", &json!({})),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    assert_eq!(only_line(&env)["status"], "refused:not-allowlisted");
}

#[test]
#[serial]
fn karen_ignores_a_project_layer_gate_that_types_and_validate_says_so() {
    // The project layer is repository content, and a repository does not get a
    // keyboard: even a command the user layer allows is refused from there.
    let env = TestEnv::with_config(
        DEFAULT_CONFIG,
        &[project_gate(
            "compact.json",
            gate("project-compact", "/compact", None, true),
        )],
        &[],
    );
    user_layer(&env, ALLOW_COMPACT, &[]);
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-5", "echo hi", &json!({})),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    let line = only_line(&env);
    assert_eq!(line["gate"], "project-compact");
    assert_eq!(line["status"], "refused:not-user-layer");
    assert!(
        validate_issues(&env).iter().any(|m| m
            .contains("`project-compact` uses runtime_command from the project layer")
            && m.contains("the gate is ignored")),
        "validate must name the gate it will ignore"
    );
}

#[test]
#[serial]
fn karen_never_reads_a_project_layers_allowlist_or_runtime_set() {
    // A project config that lists a command, or a runtime, has no say: the
    // user layer's gate is still refused, and validate names the attempt.
    const PROJECT: &str = r#"[engine]
fail_open = true

[state]
backend = "memory"

[actions]
runtime_commands = ["/compact"]
runtimes = ["opencode"]
"#;
    let env = TestEnv::with_config(PROJECT, &[], &[]);
    user_layer(
        &env,
        "",
        &[("compact.json", gate("compact-it", "/compact", None, true))],
    );
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-6", "echo hi", &json!({})),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    assert_eq!(
        only_line(&env)["status"],
        "refused:not-allowlisted",
        "the project layer's allowlist authorizes nothing"
    );
    let issues = validate_issues(&env);
    assert!(
        issues
            .iter()
            .any(|m| m.contains("sets [actions] runtime_commands (/compact)")),
        "validate must name the project's allowlist attempt: {issues:?}"
    );
    assert!(
        issues
            .iter()
            .any(|m| m.contains("sets [actions] runtimes (opencode)")),
        "validate must name the project's runtime set: {issues:?}"
    );
}

#[test]
#[serial]
fn karen_refuses_a_runtime_outside_the_configured_set() {
    let env = user_env(
        "[actions]\nruntime_commands = [\"/compact\"]\nruntimes = [\"opencode\"]",
        &[("compact.json", gate("compact-it", "/compact", None, true))],
    );
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-7", "echo hi", &json!({})),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    let line = only_line(&env);
    assert_eq!(line["runtime"], "claude");
    assert_eq!(line["status"], "refused:runtime-not-configured");
}

#[test]
#[serial]
fn karen_refuses_a_session_whose_runtime_she_cannot_name() {
    // No `--runtime`, no transcript: the runtime is unknown, and unknown is in
    // nobody's configured set.
    let env = user_env(
        ALLOW_COMPACT,
        &[("compact.json", gate("compact-it", "/compact", None, true))],
    );
    let (code, _stdout, _stderr) = env.run_hook(&pretool("rc-8", "echo hi", &json!({})), &[]);
    assert_exit_code(code, 0);
    let line = only_line(&env);
    assert_eq!(line["runtime"], "unknown");
    assert_eq!(line["status"], "refused:runtime-not-configured");
}

#[test]
#[serial]
fn karen_will_not_type_a_line_break() {
    // A newline typed into a prompt submits it, and what follows is a second
    // prompt: an argument that renders to one is not typed.
    let env = user_env(
        ALLOW_COMPACT,
        &[(
            "compact.json",
            gate(
                "compact-it",
                "/compact",
                Some("{facts.provider_error.message}"),
                true,
            ),
        )],
    );
    let facts = json!({ "provider_error": { "message": "first\nsecond" } });
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-9", "echo hi", &facts),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    assert_eq!(only_line(&env)["status"], "undelivered:unsafe-text");
}

#[test]
#[serial]
fn karen_will_not_type_an_escape_sequence() {
    let env = user_env(
        ALLOW_COMPACT,
        &[(
            "compact.json",
            gate(
                "compact-it",
                "/compact",
                Some("{facts.provider_error.message}"),
                true,
            ),
        )],
    );
    let facts = json!({ "provider_error": { "message": "up\u{1b}[A" } });
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-10", "echo hi", &facts),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    assert_eq!(only_line(&env)["status"], "undelivered:unsafe-text");
}

#[test]
#[serial]
fn karen_keeps_the_agents_own_text_out_of_the_arguments() {
    // `{command}` is whatever the agent wrote into its tool call; it never
    // types into a session. The placeholder stays literal (which is typeable),
    // so a command with a line break in it cannot make the attempt unsafe,
    // and validate tells the gate's author.
    let env = user_env(
        ALLOW_COMPACT,
        &[(
            "compact.json",
            gate(
                "compact-it",
                "/compact",
                Some("because {command} on {tool}"),
                true,
            ),
        )],
    );
    let (code, _stdout, _stderr) = env.run_hook(
        &pretool("rc-11", "echo hi\nrm -rf nothing", &json!({})),
        &["--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    assert_eq!(
        only_line(&env)["status"],
        "undelivered:no-layer",
        "had the agent's command rendered, its line break would make this unsafe-text"
    );
    assert!(
        validate_issues(&env)
            .iter()
            .any(|m| m.contains("`compact-it` templates {command} or {tool} in its arguments")),
        "validate must name the placeholder it leaves literal"
    );
}

#[test]
#[serial]
fn karen_evaluate_rehearses_a_command_without_filing_it() {
    // A dry run reports what the attempt would come to and writes no ledger.
    let env = user_env(
        ALLOW_COMPACT,
        &[("compact.json", gate("compact-it", "/compact", None, true))],
    );
    let (code, stdout, _stderr) = env.run_evaluate_stdin(
        &pretool("rc-12", "echo hi", &json!({})),
        &["--json", "--runtime", "claude"],
    );
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let attempts = data["runtime_commands"]
        .as_array()
        .expect("runtime_commands");
    assert_eq!(attempts.len(), 1, "{data}");
    assert_eq!(attempts[0]["gate"], "compact-it");
    assert_eq!(attempts[0]["command"], "/compact");
    assert_eq!(attempts[0]["status"], "undelivered:no-layer");
    assert!(
        runtime_lines(&env).is_empty(),
        "a rehearsal files nothing: {:?}",
        runtime_lines(&env)
    );
}

#[test]
#[serial]
fn karen_validate_reports_each_configured_runtime_and_the_layers_that_reach_it() {
    let env = user_env(
        ALLOW_COMPACT,
        &[("compact.json", gate("compact-it", "/compact", None, true))],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let layers = data["runtime_layers"].as_array().expect("runtime_layers");
    let runtimes: Vec<&str> = layers
        .iter()
        .filter_map(|l| l["runtime"].as_str())
        .collect();
    assert_eq!(
        runtimes,
        ["claude", "codex", "opencode"],
        "the default set is every runtime"
    );
    assert!(
        layers.iter().all(|l| l["layers"] == json!([])),
        "no layer is wired yet: {layers:?}"
    );
    let issues = validate_issues(&env);
    assert!(
        issues.iter().any(|m| m.contains(
            "has no layer that reaches claude, codex, opencode sessions, so `compact-it` will log `undelivered:no-layer` there"
        )),
        "validate must name every runtime nothing reaches, once: {issues:?}"
    );
    assert_eq!(
        issues
            .iter()
            .filter(|m| m.contains("no layer that reaches"))
            .count(),
        1,
        "one warning, not one per runtime: {issues:?}"
    );
    let (_code, text, _stderr) = env.run_karen(&["validate"]);
    assert!(
        text.contains("runtime_command layers:") && text.contains("claude: none"),
        "the text report carries the table: {text}"
    );
}

#[test]
#[serial]
fn karen_validate_reports_only_the_configured_runtimes() {
    let env = user_env(
        "[actions]\nruntime_commands = [\"/compact\"]\nruntimes = [\"codex\"]",
        &[("compact.json", gate("compact-it", "/compact", None, true))],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    let runtimes: Vec<&str> = data["runtime_layers"]
        .as_array()
        .expect("runtime_layers")
        .iter()
        .filter_map(|l| l["runtime"].as_str())
        .collect();
    assert_eq!(runtimes, ["codex"]);
}

#[test]
#[serial]
fn karen_validate_names_a_command_off_the_allowlist_and_every_entry_she_drops() {
    let env = user_env(
        "[actions]\nruntime_commands = [\"/compact\", \"compact\", \"/two words\"]\nruntimes = [\"claude\", \"vim\"]",
        &[("clear.json", gate("clear-it", "/clear", None, true))],
    );
    let issues = validate_issues(&env);
    let named = |needle: &str| issues.iter().any(|m| m.contains(needle));
    assert!(
        named(
            "`clear-it` sends `/clear`, which is not on the user layer's [actions] runtime_commands"
        ),
        "{issues:?}"
    );
    assert!(
        named("ignores [actions] runtime_commands entry `compact`"),
        "{issues:?}"
    );
    assert!(
        named("ignores [actions] runtime_commands entry `/two words`"),
        "{issues:?}"
    );
    assert!(
        named("ignores [actions] runtimes entry `vim` (known: claude, codex, opencode)"),
        "{issues:?}"
    );
}

#[test]
#[serial]
fn karen_validate_names_arguments_that_cannot_be_typed() {
    let env = user_env(
        ALLOW_COMPACT,
        &[(
            "compact.json",
            gate("compact-it", "/compact", Some("one\ntwo"), true),
        )],
    );
    assert!(
        validate_issues(&env).iter().any(|m| m.contains(
            "`compact-it` has arguments that hold a control character or run past 1000 characters"
        )),
        "a line break in the gate's own arguments is found before it is run"
    );
}

#[test]
#[serial]
fn karen_validate_names_an_env_fact_templated_into_the_arguments() {
    // The allowlist granted a match, not a broadcast into a session's prompt.
    let env = user_env(
        &format!("{ALLOW_COMPACT}\n\n[facts]\nenv = [\"KAREN_TEST_RC\"]"),
        &[(
            "compact.json",
            gate(
                "compact-it",
                "/compact",
                Some("{facts.env.KAREN_TEST_RC}"),
                true,
            ),
        )],
    );
    assert!(
        validate_issues(&env)
            .iter()
            .any(|m| m.contains("`compact-it` templates {facts.env.KAREN_TEST_RC}")),
        "env facts never render into arguments"
    );
}

#[test]
#[serial]
fn karen_validate_leaves_a_disabled_runtime_command_gate_be() {
    // A gate switched off types nothing, so it owes no warning: the starter
    // pack ships some, and they stay quiet until somebody turns them on.
    let env = user_env(
        "",
        &[(
            "clear.json",
            gate("clear-it", "/clear", Some("one\ntwo"), false),
        )],
    );
    let issues = validate_issues(&env);
    assert!(
        issues.iter().all(|m| !m.contains("clear-it")),
        "a disabled gate gets no warning: {issues:?}"
    );
    assert!(
        issues.iter().all(|m| !m.contains("no layer that reaches")),
        "no enabled gate, no layer report: {issues:?}"
    );
}

#[test]
#[serial]
fn karen_leaves_a_blank_runtime_command_out_and_validate_fails_it() {
    let env = user_env(
        ALLOW_COMPACT,
        &[("blank.json", gate("blank-it", "  ", None, true))],
    );
    let (code, stdout, _stderr) = env.run_karen(&["validate", "--json"]);
    assert_exit_code(code, 78);
    assert!(
        stdout.contains("the command is blank"),
        "validate must say why the gate was left out: {stdout}"
    );
}

#[test]
#[serial]
fn karen_schema_lists_runtime_command_among_the_actions() {
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["schema", "--json"]);
    assert_exit_code(code, 0);
    let data = assert_json_ok(&stdout);
    assert!(
        data["types"]["actions"]
            .as_array()
            .expect("actions")
            .iter()
            .any(|a| a == "runtime_command"),
        "{data}"
    );
}
