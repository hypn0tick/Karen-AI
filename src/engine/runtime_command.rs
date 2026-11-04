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
use crate::types::{CommandOutcome, EngineRequest};
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
pub fn layers_for(_runtime: &str) -> &'static [&'static str] {
    &[]
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
    let status = decide(gate, command, arguments, req, actions);
    let outcome = CommandOutcome {
        gate: gate.id.clone(),
        runtime,
        command: command.to_string(),
        layer: None,
        status: status.to_string(),
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

/// The status, in the order her rules apply: who is asking, which command,
/// what text, and only then whether anything can carry it.
fn decide(
    gate: &Gate,
    command: &str,
    arguments: Option<&str>,
    req: &EngineRequest,
    actions: &ActionsConfig,
) -> &'static str {
    if gate.layer != Layer::User {
        return status::NOT_USER_LAYER;
    }
    if !actions.runtime_commands.iter().any(|c| c == command) {
        return status::NOT_ALLOWLISTED;
    }
    let rendered = arguments
        .map(|template| dispatcher::expand_facts(&template.replace("{gate}", &gate.id), req));
    let line_chars = command.chars().count() + rendered.as_deref().map_or(0, |a| a.chars().count());
    if !is_command(command) || !rendered.as_deref().is_none_or(is_typeable) || line_chars > MAX_LINE
    {
        return status::UNSAFE_TEXT;
    }
    status::NO_LAYER
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
    fn no_runtime_has_a_layer_until_one_is_wired() {
        for runtime in crate::policy::loader::RUNTIMES {
            assert!(layers_for(runtime).is_empty(), "{runtime}");
        }
    }
}
