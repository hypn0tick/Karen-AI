// tests/e2e/common/commands.rs
// The Karen who sets up the front desk for the tests that leave messages: a
// user layer to be trusted, gates that type, and the ledger they write to.

use crate::e2e::common::TestEnv;
use serde_json::{Value, json};

/// A `runtime_command` gate that matches every call of `event`. `enabled` is
/// the gate's own switch. A `none` condition that never excludes names the
/// facts the arguments template, so they are collected for the rendering.
pub fn command_gate(
    id: &str,
    event: &str,
    command: &str,
    arguments: Option<&str>,
    enabled: bool,
) -> String {
    let mut action = json!({ "type": "runtime_command", "command": command });
    if let Some(arguments) = arguments {
        action["arguments"] = json!(arguments);
    }
    json!({
        "id": id,
        "description": "The test gate for runtime_command.",
        "event": event,
        "enabled": enabled,
        "match": {
            "all": [ { "type": "always" } ],
            "none": [
                { "type": "field_equals", "field": "facts.session_id", "value": "__never__" },
                { "type": "field_equals", "field": "facts.provider_error", "value": "__never__" }
            ]
        },
        "action": action,
    })
    .to_string()
}

/// A user layer at `<home>/.karen/policies`: the only layer that may type.
/// `config` is appended to the layer's `config.toml`; each of `gates` is a
/// file name and the gate's JSON.
pub fn user_layer(env: &TestEnv, config: &str, gates: &[(&str, String)]) {
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

/// The runtime log a test home keeps, as parsed JSON lines.
pub fn runtime_lines(env: &TestEnv) -> Vec<Value> {
    runtime_log(env)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The runtime log's raw text, empty when nothing was ever filed.
pub fn runtime_log(env: &TestEnv) -> String {
    std::fs::read_to_string(
        env.home_dir
            .join(".karen")
            .join("logs")
            .join("karen-runtime.jsonl"),
    )
    .unwrap_or_default()
}
