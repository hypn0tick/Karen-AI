//! The dispatcher: evaluates policies and aggregates verdicts. The Karen who dispatches.

use crate::engine::jsonl::{self, now_millis};
use crate::engine::{injector, runtime_command};
use crate::policy::PolicySet;
use crate::policy::gate::{ActionDef, Gate, StateSpec};
use crate::state::StateStore;
use crate::types::{
    Action, CommandOutcome, EngineRequest, EngineResponse, Event, Injection, InjectionKind,
};
use serde_json::Value;
use std::sync::{Arc, RwLock};

pub struct Engine {
    // The filing cabinet, shared with the live-reload watcher: a read
    // lock for every ruling, a write lock only for reloads. The Karen
    // who lets exactly one trusted colleague swap the law mid-shift —
    // nobody else touches the cabinet.
    policies: Arc<RwLock<PolicySet>>,
    state: Box<dyn StateStore>,
}

impl Engine {
    #[must_use]
    pub fn new(policies: PolicySet, state: Box<dyn StateStore>) -> Self {
        Self {
            policies: Arc::new(RwLock::new(policies)),
            state,
        }
    }

    /// The current policy set, cloned fresh from the cabinet. She hands
    /// you a copy, not the original — the original is load-bearing.
    ///
    /// # Panics
    ///
    /// If the policy lock is poisoned — which takes a panic mid-reload,
    /// and a panic mid-reload is already the fire alarm. The Karen who
    /// falls rather than freezes at her desk.
    #[must_use]
    pub fn policies(&self) -> PolicySet {
        self.policies
            .read()
            .expect("policy lock poisoned — she does not freeze mid-ruling")
            .clone()
    }

    /// Crate-internal handle for the live-reload watcher. The Karen who
    /// shares her filing cabinet with one colleague, on a need-to-know basis.
    #[must_use]
    pub(crate) fn shared_policies(&self) -> Arc<RwLock<PolicySet>> {
        Arc::clone(&self.policies)
    }

    /// Swap policies while she's running. She reloads before your finger leaves Ctrl+S — and with [`Engine::watch_policies`], she reloads without being asked at all. The Karen who's already ready, manager on speed dial.
    ///
    /// # Panics
    ///
    /// If the policy lock is poisoned. The only way in is a watcher or
    /// an embedder that panicked mid-swap; she treats that as a reason
    /// to make noise, not a reason to go quiet.
    pub fn replace_policies(&mut self, policies: PolicySet) {
        *self
            .shared_policies()
            .write()
            .expect("policy lock poisoned — she does not freeze mid-ruling") = policies;
    }

    /// Evaluate a request and deliver a verdict. The Karen who judges every tool call, looking for any reason to call the manager.
    ///
    /// # Panics
    ///
    /// If the policy lock is poisoned. She would rather stop loudly
    /// than rule quietly from a cabinet she cannot trust — and the
    /// harness above her has fail-open for exactly this.
    #[must_use]
    pub fn dispatch(&self, req: &EngineRequest) -> EngineResponse {
        let mut fired = Vec::new();
        let mut injections: Vec<Injection> = Vec::new();
        let mut ask: Option<Action> = None;
        let mut modify: Option<Action> = None;
        let mut commands: Vec<CommandOutcome> = Vec::new();

        let policies = self
            .policies
            .read()
            .expect("policy lock poisoned — she does not freeze mid-ruling");

        if matches!(req.event, Event::PostCompact) {
            injector::invalidate(self.state.as_ref(), &req.session_id);
            // Compaction drops what a nudge taught, so gates that ask get to nudge once
            // more. The Karen who repeats herself only when you have actually forgotten.
            for spec in policies.gates.iter().filter_map(|g| g.state.as_ref()) {
                if spec.reset_on_compact {
                    self.state.remove(&req.session_id, &spec.key);
                }
            }
        }

        for gate in Self::applicable(&policies, req) {
            if self.held_by_latch(gate, req) {
                continue;
            }
            if !gate.matcher.matches(req) {
                continue;
            }
            fired.push(gate.id.clone());
            // A fired latch arms: the next matching dispatch is held until the
            // field falls back below the line. The value is a timestamp, so
            // the ledger says when the gate last fired, not just that it did.
            if gate.state.as_ref().and_then(StateSpec::latch).is_some() {
                self.state
                    .set(&req.session_id, &state_key(gate), Value::from(now_millis()));
            }

            // A command typed into the session is a side effect, not a
            // verdict: the call proceeds, and the outcome rides the response
            // (and the runtime log) whether or not anything carried it.
            if let ActionDef::RuntimeCommand { command, arguments } = &gate.action {
                commands.push(runtime_command::attempt(
                    gate,
                    command,
                    arguments.as_deref(),
                    req,
                    &policies.config.actions,
                    self.state.read_only(),
                ));
                continue;
            }

            match self.evaluate(gate, req) {
                Verdict::Deny(action, injection) => {
                    let mut injections = injections;
                    if let Some(inj) = injection {
                        injections.push(inj);
                    }
                    return deny_with(action, fired, injections, commands);
                }
                Verdict::Ask(action) => {
                    ask.get_or_insert(action);
                }
                Verdict::Modify(action) => {
                    modify.get_or_insert(action);
                }
                Verdict::Allow(Some(injection)) => injections.push(injection),
                Verdict::Allow(None) => {}
            }
        }

        if let Some(action) = modify {
            let modified_input = if let Action::Modify { modified_input, .. } = &action {
                modified_input.clone()
            } else {
                None
            };
            return EngineResponse {
                action,
                injections,
                modified_input,
                fired_policies: fired,
                runtime_commands: commands,
            };
        }
        if let Some(action) = ask {
            return EngineResponse {
                action,
                injections,
                modified_input: None,
                fired_policies: fired,
                runtime_commands: commands,
            };
        }

        if matches!(req.event, Event::SessionStart | Event::UserPromptSubmit) {
            if let Some(injection) = injector::rule_injection(
                &policies,
                self.state.as_ref(),
                &req.session_id,
                false,
                req.part,
                Some(&crate::engine::facts::runtime(&req.raw)),
            ) {
                injections.push(injection);
            }
        }

        EngineResponse {
            action: Action::Allow,
            injections,
            modified_input: None,
            fired_policies: fired,
            runtime_commands: commands,
        }
    }

    /// Whether a latch keeps `gate` quiet for this request. A latch re-arms on
    /// every dispatch, matched or not: a gate that fires at 40% can never match
    /// below 30%, so a latch that waited for a match to clear would hold the
    /// gate shut for good. The field falling back below the line clears it,
    /// whatever the gate matches on. Latched, and the field has not fallen back
    /// below the line: the gate already fired, and she stays quiet until it
    /// has. The Karen who resets the tripwire walking past it, not by stepping
    /// on it once more.
    fn held_by_latch(&self, gate: &Gate, req: &EngineRequest) -> bool {
        let Some((field, rearm_below)) = gate.state.as_ref().and_then(StateSpec::latch) else {
            return false;
        };
        let key = state_key(gate);
        let below_line = crate::policy::gate::lookup(req, field)
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|number| number < rearm_below);
        if below_line {
            self.state.remove(&req.session_id, &key);
        }
        self.state.get(&req.session_id, &key).is_some()
    }

    fn applicable<'a>(
        policies: &'a PolicySet,
        req: &EngineRequest,
    ) -> impl Iterator<Item = &'a Gate> {
        policies
            .gates
            .iter()
            .filter(|gate| gate.applies_to(&req.event, req.tool_name.as_deref()))
        // The Karen who filters gates like she's checking receipts at the door.
    }

    fn evaluate(&self, gate: &Gate, req: &EngineRequest) -> Verdict {
        match &gate.action {
            // Handled before a verdict is asked for (see `dispatch`): it types
            // into the session and has no verdict of its own.
            ActionDef::Allow | ActionDef::RuntimeCommand { .. } => Verdict::Allow(None),
            ActionDef::Notify { reason } => {
                let text = expand(reason, gate, req);
                // A rehearsal does not file: evaluate reads real strike state
                // so the preview is honest, and the notify log stays untouched
                // by a dry run, like every other ledger.
                if !self.state.read_only() {
                    notify_log(&req.session_id, &gate.id, &text);
                }
                // Said and stepped aside: the notice teaches in-session, the
                // log line teaches whoever tails it, and the call proceeds.
                Verdict::Allow(Some(Injection {
                    content: text,
                    kind: InjectionKind::Notice,
                    once_per_session: false,
                    source: Some(gate.id.clone()),
                }))
            }
            ActionDef::Deny { reason } => {
                Verdict::Deny(Action::deny(expand(reason, gate, req)), None)
            }
            ActionDef::Ask { reason } => Verdict::Ask(Action::ask(expand(reason, gate, req))),
            ActionDef::Modify {
                reason,
                input,
                modifier,
            } => {
                let modified_input = if let Some(modifier_name) = modifier {
                    apply_modifier(modifier_name, req, input.as_ref())
                } else {
                    input.clone()
                };
                Verdict::Modify(
                    Action::modify(expand(reason, gate, req)).with_modified_input(modified_input),
                )
            }
            ActionDef::DenyOnceThenWarn {
                first_reason,
                second_reason,
            } => {
                let spec = gate.state.as_ref();
                let key = spec.map_or(gate.id.as_str(), |s| s.key.as_str());
                let key = match spec.map(|s| s.track) {
                    Some(crate::policy::gate::Track::Input) => {
                        format!("{key}:{:016x}", input_key(spec, req))
                    }
                    _ => key.to_string(),
                };
                let max = spec.map_or(2, |s| s.max_strikes.max(1));
                let strikes = self.state.increment(&req.session_id, &key);

                if strikes < u64::from(max) {
                    // First strike: the denial and the lecture ship together — first_reason up front, the coaching in a Notice. The Karen who never wastes an opportunity to explain herself.
                    Verdict::Deny(
                        Action::deny(expand(first_reason, gate, req)),
                        Some(Injection {
                            content: expand(first_reason, gate, req),
                            kind: InjectionKind::Notice,
                            once_per_session: false,
                            source: Some(gate.id.clone()),
                        }),
                    )
                } else {
                    Verdict::Allow(Some(Injection {
                        content: expand(second_reason, gate, req),
                        kind: InjectionKind::Notice,
                        once_per_session: false,
                        source: Some(gate.id.clone()),
                    }))
                }
            }
        }
        // The Karen who evaluates every gate like it's a performance review.
    }
}

enum Verdict {
    Allow(Option<Injection>), // The Karen who steps aside, because the rules say it's fine.
    Deny(Action, Option<Injection>), // The Karen who says no, and has the policy to back it up.
    Ask(Action),              // The Karen who escalates, because you need adult supervision.
    Modify(Action),           // The Karen who gets fed up and just does it for you.
}

/// The state key a gate latches and counts under: its spec's `key`, else its id.
fn state_key(gate: &Gate) -> String {
    gate.state
        .as_ref()
        .map_or(gate.id.as_str(), |s| s.key.as_str())
        .to_string()
}

fn deny_with(
    action: Action,
    fired: Vec<String>,
    injections: Vec<Injection>,
    commands: Vec<CommandOutcome>,
) -> EngineResponse {
    let modified_input = if let Action::Modify { modified_input, .. } = &action {
        modified_input.clone()
    } else {
        None
    };
    // The Karen who delivers the verdict and makes it stick.
    EngineResponse {
        action,
        injections,
        modified_input,
        fired_policies: fired,
        runtime_commands: commands,
    }
}

fn apply_modifier(
    modifier: &str,
    req: &EngineRequest,
    input: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    match modifier {
        "head-to-select-object" => convert_head_to_select_object(req),
        _ => input.cloned(),
    }
}

fn convert_head_to_select_object(req: &EngineRequest) -> Option<serde_json::Value> {
    let command = req.command()?;

    // Match head -N, head -n N, head -nN — the Unix habits she confiscates at the border.
    let re = regex::Regex::new(r"(?i)head\s+(?:-n\s*)?-?(\d+)").ok()?;
    let caps = re.captures(command)?;
    let n = caps.get(1)?.as_str().parse::<usize>().ok()?;

    // Determine if it's a pipeline or file
    if command.contains('|') {
        // Pipeline: cmd | head -N -> cmd | Select-Object -First N
        let converted = regex::Regex::new(r"(?i)\s*\|\s*head\s+(?:-n\s*)?-?\d+")
            .ok()?
            .replace(command, &format!(" | Select-Object -First {n}"));
        return Some(serde_json::json!({"command": converted.to_string()}));
    }

    // File: head -N file.txt -> Get-Content file.txt | Select-Object -First N
    // The Karen who knows which way the pipe points, and grades accordingly.
    let file_re = regex::Regex::new(r"(?i)head\s+(?:-n\s*)?-?\d+\s+(\S+)").ok()?;
    let caps = file_re.captures(command)?;
    let file = caps.get(1)?.as_str();
    let converted = format!("Get-Content {file} | Select-Object -First {n}");
    Some(serde_json::json!({"command": converted}))
}

/// Expand `{gate}`, `{tool}` and `{command}` as ever, plus `{facts.<path>}`:
/// the rendered fact from the enriched request. Absent renders as
/// `(unknown)` — an empty string would read like a value — an object or array
/// as its compact JSON, a number as its JSON number. The one fact never
/// rendered is `facts.env.*`: the allowlist granted a *match*, not a
/// broadcast into reasons, notices and the notify log. The Karen who quotes
/// the record, never the lockbox.
fn expand(template: &str, gate: &Gate, req: &EngineRequest) -> String {
    // The Karen who remembers EXACTLY what she wants, and will certainly demand it.
    let text = template
        .replace("{gate}", &gate.id)
        .replace("{tool}", req.tool_name.as_deref().unwrap_or("unknown"))
        .replace("{command}", req.command().unwrap_or(""));
    expand_facts(&text, req)
}

/// The `{facts.<path>}` half of [`expand`], for text that must not carry the
/// agent's own `{tool}` and `{command}`: the arguments a `runtime_command`
/// types into a session.
pub(crate) fn expand_facts(template: &str, req: &EngineRequest) -> String {
    let mut out = template.to_string();
    let mut from = 0;
    while let Some(open) = out[from..].find("{facts.") {
        let open = from + open;
        let Some(close) = out[open..].find('}') else {
            break;
        };
        let path = &out[open + "{facts.".len()..open + close];
        let rendered = if path.starts_with("env.") {
            // Not "(unknown)": the literal text, so the author sees their own
            // placeholder refused in place. The search moves past it — the
            // literal would otherwise be found and refused again, forever.
            format!("{{facts.{path}}}")
        } else {
            crate::policy::gate::lookup(req, &format!("facts.{path}"))
                .map_or_else(|| "(unknown)".into(), render_fact)
        };
        let end = open + close + 1;
        out.replace_range(open..end, &rendered);
        // Past the rendered span: a fact's own text is never re-expanded.
        from = open + rendered.len();
    }
    out
}

/// A fact for an author's eyes: a string as itself, anything else as its
/// compact JSON, so a number stays a number and an object stays readable.
fn render_fact(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// One line of the notify log, under `<Karen home>/logs/karen-notify.jsonl`:
/// `{"time": <epoch-ms>, "session": ..., "gate": ..., "text": ...}`. Past
/// 10 MiB the log rotates — one generation, `karen-notify.jsonl.1` — so a
/// long-running runtime does not grow a log without end. The Karen who files
/// every complaint, and empties the drawer when it sticks.
fn notify_log(session: &str, gate: &str, text: &str) {
    jsonl::append(
        "karen-notify.jsonl",
        &serde_json::json!({
            "time": now_millis(),
            "session": session,
            "gate": gate,
            "text": jsonl::one_line(text),
        }),
    );
}

/// The part of a tool input that decides what runs, for an input-tracked strike
/// count. A shell call arrives with a free-text `description` of the command
/// (the shell tools of Claude Code and `OpenCode` both send one), and the same command in other words
/// is still the same command, so the note is dropped when a string `command`
/// sits beside it. Without a `command` the `description` is part of the payload
/// (an issue tracker's `create_issue`), and it stays in the key. Every other
/// field, such as `timeout` or `workdir`, changes what runs and stays too.
fn strike_input(tool_input: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let mut input = tool_input?.clone();
    if let Some(fields) = input.as_object_mut() {
        if fields
            .get("command")
            .is_some_and(serde_json::Value::is_string)
        {
            fields.remove("description");
        }
    }
    Some(input)
}

/// What "the same input" means for one gate's input-tracked strike: the fields
/// its author listed in `key_fields`, else the default of [`strike_input`].
/// A listed field that is missing hashes as a path with no value, so absent,
/// `""` and `null` are three different inputs: a gate that keys on a field the
/// call left out has not been told the field is empty.
fn input_key(spec: Option<&StateSpec>, req: &EngineRequest) -> u64 {
    let Some(fields) = spec.and_then(|s| s.key_fields.as_ref()) else {
        return fnv1a(strike_input(req.raw.get("tool_input")).as_ref());
    };
    let picked: Vec<Value> = fields
        .iter()
        .map(|path| match crate::policy::gate::lookup(req, path) {
            Some(value) => serde_json::json!([path, value]),
            None => serde_json::json!([path]),
        })
        .collect();
    fnv1a(Some(&Value::Array(picked)))
}

/// FNV-1a over a value's JSON text: a strike key that is stable across runs and
/// builds, which `std`'s hasher does not promise.
fn fnv1a(value: Option<&serde_json::Value>) -> u64 {
    let text = value.map(ToString::to_string).unwrap_or_default();
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}
