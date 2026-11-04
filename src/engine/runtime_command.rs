//! The `runtime_command` action: type a slash command into a session, below
//! the runtime's hook API. A hook can answer a tool call; none can start
//! `/compact`, so a command reaches a session through whichever layer it has
//! (the runtime's own plugin API, or a terminal Karen can write to), and
//! through nothing else.
//!
//! Every attempt ends as exactly one status, is returned in the verdict, and is
//! written to `<Karen home>/logs/karen-runtime.jsonl` unless it is a rehearsal:
//!
//! - `refused:not-user-layer`: the gate is not the user layer's. A project's
//!   gates are repository content, and a repository does not get a keyboard.
//! - `refused:not-allowlisted`: the command is not on the user layer's
//!   `[actions] runtime_commands`.
//! - `undelivered:unsafe-text`: the command or its rendered arguments hold a
//!   control character or run past [`MAX_LINE`]. A newline typed into a prompt
//!   is a second prompt, and an escape sequence is a keystroke.
//! - `undelivered:no-layer`: allowed, and nothing reaches this session.
//! - `undelivered:host-unreachable` / `undelivered:host-refused`: the terminal
//!   host (`karen run`) was named by the session's environment and did not answer,
//!   or answered that it would not take the line.
//! - `handed-off`: allowed, and a layer took it (named in `layer`). The layer
//!   delivers after her process has exited, so this is the last thing she can
//!   know; the layer writes what became of it to the same log (the host files
//!   `sent`, `undelivered:timeout`, `undelivered:host-exited` and
//!   `undelivered:type-failed`).
//!
//! A delivery is attempted on any runtime that has a layer. The user layer's
//! `[actions] runtimes` only scopes what `karen validate` reports: a list
//! entry forgotten must not quietly switch a reachable runtime off.
//!
//! The log carries the command and the arguments as the gate wrote them,
//! placeholders unexpanded: what a session's facts rendered into them is data
//! about the session, and the ledger does not keep it.
//!
//! The Karen who asks the front desk to relay the message, and writes down who
//! answered the phone.

use crate::engine::{dispatcher, facts, jsonl};
use crate::policy::gate::{Gate, Layer};
use crate::policy::loader::ActionsConfig;
use crate::types::{CommandOutcome, EngineRequest, TypedCommand};
use serde_json::json;

/// The runtime log, under `<Karen home>/logs`.
pub const LOG: &str = "karen-runtime.jsonl";

/// The longest line she will type: a command, a space and its arguments. A
/// prompt line has to be one a person could have typed.
pub const MAX_LINE: usize = 1000;

/// The statuses an attempt ends as.
pub mod status {
    pub const NOT_USER_LAYER: &str = "refused:not-user-layer";
    pub const NOT_ALLOWLISTED: &str = "refused:not-allowlisted";
    pub const UNSAFE_TEXT: &str = "undelivered:unsafe-text";
    pub const NO_LAYER: &str = "undelivered:no-layer";
    pub const HANDED_OFF: &str = "handed-off";
    pub const HOST_UNREACHABLE: &str = "undelivered:host-unreachable";
    pub const HOST_REFUSED: &str = "undelivered:host-refused";
    /// Filed by a layer after the fact, not decided by her.
    pub const SENT: &str = "sent";
    pub const TIMEOUT: &str = "undelivered:timeout";
    pub const HOST_EXITED: &str = "undelivered:host-exited";
    pub const TYPE_FAILED: &str = "undelivered:type-failed";
}

/// The layers a command can ride, by the name a log line carries.
pub mod layer {
    /// `OpenCode`'s own server API, called from the bridge plugin: a custom
    /// command through `session.command`, `/compact` through
    /// `session.summarize`, a TUI built-in through `tui.executeCommand`.
    pub const OPENCODE_NATIVE: &str = "opencode-native";
    /// A session run under `karen run`: the host types into its terminal once it is idle.
    pub const TERMINAL_HOST: &str = "terminal-host";
}

/// Whether `text` can be typed as part of one prompt line: no control
/// character (a newline submits, an escape is a key) and no longer than
/// [`MAX_LINE`].
#[must_use]
pub fn is_typeable(text: &str) -> bool {
    text.chars().count() <= MAX_LINE && !text.chars().any(char::is_control)
}

/// Whether `text` is a command she may be given: `/` and a name, one word,
/// typeable. The arguments are the gate's, never part of the command.
#[must_use]
pub fn is_command(text: &str) -> bool {
    text.len() > 1
        && text.starts_with('/')
        && is_typeable(text)
        && !text.chars().any(char::is_whitespace)
}

/// The layers that can carry a command into a `runtime`'s sessions, best
/// first. A session reaches one of them or none, and none is logged.
#[must_use]
pub fn layers_for(runtime: &str) -> &'static [&'static str] {
    #[cfg(feature = "host")]
    const TERMINAL: &[&str] = &[layer::TERMINAL_HOST];
    #[cfg(not(feature = "host"))]
    const TERMINAL: &[&str] = &[];
    #[cfg(feature = "host")]
    const OPENCODE: &[&str] = &[layer::OPENCODE_NATIVE, layer::TERMINAL_HOST];
    #[cfg(not(feature = "host"))]
    const OPENCODE: &[&str] = &[layer::OPENCODE_NATIVE];
    match runtime {
        "opencode" => OPENCODE,
        "claude" | "codex" => TERMINAL,
        _ => &[],
    }
}

/// What has to be true for a layer to reach a session, in words, for `karen validate`.
#[must_use]
pub fn reach_condition(layer: &str) -> &'static str {
    match layer {
        layer::OPENCODE_NATIVE => "when the bridge plugin is current (`karen setup opencode`)",
        layer::TERMINAL_HOST => "only in a session started with `karen run`",
        _ => "",
    }
}

/// The host a session runs under, when the environment names one for this runtime: its endpoint
/// and token. A host for another runtime is not this session's keyboard.
#[cfg(feature = "host")]
fn host_for(runtime: &str) -> Option<(String, String)> {
    let target = crate::host::endpoint::from_env()?;
    let hosted = std::env::var(crate::host::endpoint::ENV_RUNTIME).ok()?;
    (hosted == runtime).then_some(target)
}

#[cfg(not(feature = "host"))]
fn host_for(_runtime: &str) -> Option<(String, String)> {
    None
}

/// Give the rendered line to the host. It answers at once with a place in its queue; what becomes of
/// the line after that is the host's to file. A host that does not answer in time is unreachable.
#[cfg(feature = "host")]
fn hand_to_host(
    target: (String, String),
    text: String,
    meta: crate::host::endpoint::DeliveryMeta,
) -> &'static str {
    use crate::host::endpoint::{Request, call};
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(call(&target.0, &target.1, &Request::Deliver { text, meta }));
    });
    match receiver.recv_timeout(std::time::Duration::from_secs(3)) {
        Ok(Ok(answer)) if answer.ok => status::HANDED_OFF,
        Ok(Ok(_)) => status::HOST_REFUSED,
        _ => status::HOST_UNREACHABLE,
    }
}

/// The layers the payload says its caller can run, from the top-level
/// `runtime_layers` list the `OpenCode` bridge sends. A layer that lives in the
/// runtime's own process is the caller's to offer: a bridge from before the
/// layer existed offers nothing, and the command is `undelivered:no-layer`
/// instead of a hand-off nobody picks up.
fn offered(raw: &serde_json::Value) -> Vec<&str> {
    raw.get("runtime_layers")
        .and_then(serde_json::Value::as_array)
        .map(|names| names.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default()
}

/// Decide what becomes of one `runtime_command` that fired, record it, and
/// return the outcome. `rehearsal` decides and reports but writes no log line:
/// a dry run does not file.
#[must_use]
pub fn attempt(
    gate: &Gate,
    command: &str,
    arguments: Option<&str>,
    req: &EngineRequest,
    actions: &ActionsConfig,
    rehearsal: bool,
) -> CommandOutcome {
    let runtime = facts::runtime(&req.raw);
    let host = host_for(&runtime);
    #[cfg_attr(not(feature = "host"), allow(unused_mut))]
    let mut decision = decide(gate, command, arguments, req, actions, host.is_some());
    #[cfg(feature = "host")]
    if let (Some(target), Some(line), false) = (host, decision.line.take(), rehearsal) {
        if decision.layer == Some(layer::TERMINAL_HOST) {
            let meta = crate::host::endpoint::DeliveryMeta {
                session: req.session_id.clone(),
                gate: gate.id.clone(),
                runtime: runtime.clone(),
                command: command.to_string(),
                arguments: arguments.map(str::to_string),
            };
            decision.status = hand_to_host(target, line, meta);
        }
    }
    let outcome = CommandOutcome {
        gate: gate.id.clone(),
        runtime,
        command: command.to_string(),
        layer: decision.layer.map(str::to_string),
        status: decision.status.to_string(),
        typed: decision.typed,
    };
    if !rehearsal {
        jsonl::append(
            LOG,
            &json!({
                "time": jsonl::now_millis(),
                "session": req.session_id,
                "gate": outcome.gate,
                "runtime": outcome.runtime,
                "command": outcome.command,
                "arguments": arguments,
                "layer": outcome.layer,
                "status": outcome.status,
            }),
        );
    }
    outcome
}

/// What `decide` settled: the status, the layer that took the command when one
/// did, and the line it is to type.
struct Decision {
    status: &'static str,
    layer: Option<&'static str>,
    /// What the caller's own layer is to type (the `OpenCode` bridge types for itself).
    typed: Option<TypedCommand>,
    /// The whole line, for a layer she hands it to herself (the terminal host).
    #[cfg_attr(not(feature = "host"), allow(dead_code))]
    line: Option<String>,
}

impl Decision {
    /// Ended here, with nothing handed to anyone.
    const fn stop(status: &'static str) -> Self {
        Self {
            status,
            layer: None,
            typed: None,
            line: None,
        }
    }
}

/// The decision, in the order her rules apply: who is asking, which command,
/// what text, and only then whether anything can carry it.
fn decide(
    gate: &Gate,
    command: &str,
    arguments: Option<&str>,
    req: &EngineRequest,
    actions: &ActionsConfig,
    hosted: bool,
) -> Decision {
    if gate.layer != Layer::User {
        return Decision::stop(status::NOT_USER_LAYER);
    }
    if !actions.runtime_commands.iter().any(|c| c == command) {
        return Decision::stop(status::NOT_ALLOWLISTED);
    }
    let rendered = arguments
        .map(|template| dispatcher::expand_facts(&template.replace("{gate}", &gate.id), req));
    let line_chars = command.chars().count() + rendered.as_deref().map_or(0, |a| a.chars().count());
    if !is_command(command) || !rendered.as_deref().is_none_or(is_typeable) || line_chars > MAX_LINE
    {
        return Decision::stop(status::UNSAFE_TEXT);
    }
    let offered = offered(&req.raw);
    let carrier =
        layers_for(&facts::runtime(&req.raw))
            .iter()
            .copied()
            .find(|layer| match *layer {
                layer::OPENCODE_NATIVE => offered.contains(layer),
                layer::TERMINAL_HOST => hosted,
                _ => false,
            });
    let rendered = rendered.unwrap_or_default();
    match carrier {
        Some(layer::TERMINAL_HOST) => Decision {
            status: status::HANDED_OFF,
            layer: Some(layer::TERMINAL_HOST),
            typed: None,
            line: Some(if rendered.is_empty() {
                command.to_string()
            } else {
                format!("{command} {rendered}")
            }),
        },
        Some(layer) => Decision {
            status: status::HANDED_OFF,
            layer: Some(layer),
            typed: Some(TypedCommand {
                command: command.to_string(),
                arguments: rendered,
            }),
            line: None,
        },
        None => Decision::stop(status::NO_LAYER),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_a_slash_and_one_word() {
        assert!(is_command("/compact"));
        assert!(is_command("/dcp-compress"));
        assert!(!is_command("/"), "a slash alone is not a command");
        assert!(!is_command("compact"), "a command begins with a slash");
        assert!(!is_command("/compact now"), "arguments belong to the gate");
        assert!(!is_command("/compact\n"), "a newline submits");
        assert!(!is_command(""), "nothing is not a command");
    }

    #[test]
    fn typeable_text_is_one_printable_line() {
        assert!(is_typeable(""));
        assert!(is_typeable("keep the open items and the shas"));
        assert!(is_typeable("naïve café ✓"));
        assert!(!is_typeable("one\ntwo"));
        assert!(
            !is_typeable("a\u{1b}[A"),
            "an escape sequence is a keystroke"
        );
        assert!(!is_typeable("tab\there"));
        assert!(is_typeable(&"x".repeat(MAX_LINE)));
        assert!(!is_typeable(&"x".repeat(MAX_LINE + 1)));
    }

    #[test]
    fn each_runtime_has_the_layers_that_can_reach_it() {
        assert_eq!(layers_for("unknown").len(), 0);
        if cfg!(feature = "host") {
            assert_eq!(layers_for("claude"), [layer::TERMINAL_HOST]);
            assert_eq!(layers_for("codex"), [layer::TERMINAL_HOST]);
            assert_eq!(
                layers_for("opencode"),
                [layer::OPENCODE_NATIVE, layer::TERMINAL_HOST],
                "the native layer is preferred"
            );
        } else {
            assert_eq!(layers_for("claude").len(), 0);
            assert_eq!(layers_for("opencode"), [layer::OPENCODE_NATIVE]);
        }
    }

    #[test]
    fn a_layer_is_offered_by_the_payload_or_not_at_all() {
        assert_eq!(
            offered(&json!({ "runtime_layers": ["opencode-native", 7] })),
            ["opencode-native"],
            "only strings count"
        );
        let none = Vec::<&str>::new();
        assert_eq!(offered(&json!({})), none);
        assert_eq!(
            offered(&json!({ "runtime_layers": "opencode-native" })),
            none
        );
    }
}
