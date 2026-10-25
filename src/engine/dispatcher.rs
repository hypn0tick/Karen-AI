//! The dispatcher: evaluates policies and aggregates verdicts. The Karen who dispatches.

use crate::engine::injector;
use crate::policy::PolicySet;
use crate::policy::gate::{ActionDef, Gate};
use crate::state::StateStore;
use crate::types::{Action, EngineRequest, EngineResponse, Event, Injection, InjectionKind};
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

        if matches!(req.event, Event::PostCompact) {
            injector::invalidate(self.state.as_ref(), &req.session_id);
        }

        let policies = self
            .policies
            .read()
            .expect("policy lock poisoned — she does not freeze mid-ruling");

        for gate in Self::applicable(&policies, req) {
            if !gate.matcher.matches(req) {
                continue;
            }
            fired.push(gate.id.clone());

            match self.evaluate(gate, req) {
                Verdict::Deny(action, injection) => {
                    let mut injections = injections;
                    if let Some(inj) = injection {
                        injections.push(inj);
                    }
                    return deny_with(action, fired, injections);
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
            };
        }
        if let Some(action) = ask {
            return EngineResponse {
                action,
                injections,
                modified_input: None,
                fired_policies: fired,
            };
        }

        if matches!(req.event, Event::SessionStart | Event::UserPromptSubmit) {
            if let Some(injection) = injector::rule_injection(
                &policies,
                self.state.as_ref(),
                &req.session_id,
                false,
                req.part,
            ) {
                injections.push(injection);
            }
        }

        EngineResponse {
            action: Action::Allow,
            injections,
            modified_input: None,
            fired_policies: fired,
        }
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
            ActionDef::Allow => Verdict::Allow(None),
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
                let max = spec.map_or(2, |s| s.max_strikes.max(1));
                let strikes = self.state.increment(&req.session_id, key);

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

fn deny_with(action: Action, fired: Vec<String>, injections: Vec<Injection>) -> EngineResponse {
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

fn expand(template: &str, gate: &Gate, req: &EngineRequest) -> String {
    // The Karen who remembers EXACTLY what she wants, and will certainly demand it.
    template
        .replace("{gate}", &gate.id)
        .replace("{tool}", req.tool_name.as_deref().unwrap_or("unknown"))
        .replace("{command}", req.command().unwrap_or(""))
}
