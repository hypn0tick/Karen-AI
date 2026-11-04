//! Gates: the JSON/JSONC/JSON5 you write, the verdicts she delivers. Four answers: allow, deny, ask, modify. No maybe. The Karen who decides.

use crate::types::Event;
use regex::Regex;
use serde::Deserialize;
use std::fmt;
use std::sync::OnceLock;

/// Raw gate as written in your gate file. The Karen who reads your drafts, has notes, and is still looking for any excuse to call your manager.
///
/// Unknown keys are refused by name: a typo'd `match` once silently
/// became a matcher that matched everything, and a gate that fires on
/// the whole world because of a spelling error is an incident wearing
/// a typo's clothes. She checks the spelling before she checks you.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateDef {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Event name, e.g. `PreToolUse`, `PostToolUse`, `UserPromptSubmit`. The Karen who listens.
    pub event: String,
    /// Optional tool filter: an exact tool name (`Bash`, `Edit`, ...), matched
    /// case-insensitively, or a regex over it (`Edit|Write`). It must compile
    /// as a regex either way. The Karen who filters.
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub r#match: MatchExpr,
    pub action: ActionDef,
    #[serde(default)]
    pub state: Option<StateSpec>,
    /// Replace the gate of the same id from an earlier layer. Without it a
    /// reused id is refused, so no gate is ever shadowed by accident.
    #[serde(default, rename = "override")]
    pub replaces: bool,
    /// A later layer can neither replace nor disable this gate.
    #[serde(default)]
    pub locked: bool,
}

/// The policy layer a gate came from, in merge order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Builtin,
    User,
    Project,
}

impl Layer {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

const fn default_priority() -> i32 {
    50
}

const fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchExpr {
    #[serde(default)]
    pub all: Vec<ConditionDef>,
    #[serde(default)]
    pub any: Vec<ConditionDef>,
    #[serde(default)]
    pub none: Vec<ConditionDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConditionDef {
    CommandRegex {
        pattern: String,
    },
    CommandContains {
        value: String,
    },
    FileRegex {
        pattern: String,
    },
    PathRegex {
        pattern: String,
    },
    PromptRegex {
        pattern: String,
    },
    ContentRegex {
        pattern: String,
    },
    ToolRegex {
        pattern: String,
    },
    FieldEquals {
        field: String,
        value: serde_json::Value,
    },
    FieldRegex {
        field: String,
        pattern: String,
    },
    /// `field_regex` for any value: a string as itself, anything else as its
    /// compact JSON text. Opt-in, so a `field_regex` written for a string never
    /// starts matching an object (Edit's `tool_response` holds the whole file).
    FieldJsonRegex {
        field: String,
        pattern: String,
    },
    /// The field's number is at least `value`. Absence is not a number: a
    /// missing field, or one that is not numeric, never matches. The Karen who
    /// reads the meter before she quotes the ordinance.
    FieldGte {
        field: String,
        value: f64,
    },
    /// The field's number is at most `value`, with [`ConditionDef::FieldGte`]'s
    /// stand on absence.
    FieldLte {
        field: String,
        value: f64,
    },
    /// Matches when the file at `path` is missing or was last modified more
    /// than `seconds` ago. `~/` expands to the home directory; a relative path
    /// resolves against the payload's `cwd`.
    FileOlderThan {
        path: String,
        seconds: u64,
    },
    Always,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionDef {
    Allow,
    Deny {
        reason: String,
    },
    Ask {
        reason: String,
    },
    Modify {
        reason: String,
        #[serde(default)]
        input: Option<serde_json::Value>,
        #[serde(default)]
        modifier: Option<String>,
    },
    DenyOnceThenWarn {
        first_reason: String,
        second_reason: String,
    },
    /// Say it and stay out of the way: the reason renders into the session as
    /// a notice and lands as one line in the notify log, and the call
    /// proceeds. The Karen who files the complaint and lets you through,
    /// because the complaint was the point.
    Notify {
        reason: String,
    },
    /// Type a slash command into the session, below the runtime's hook API:
    /// delivered by the best layer the session has, and the call proceeds
    /// whether or not one exists. User layer only, and only for a command on
    /// the user layer's `[actions] runtime_commands` allowlist; every attempt
    /// is written to the runtime log with the layer that carried it. The
    /// `arguments` template renders `{gate}` and `{facts.<path>}` (never the
    /// agent's own `{command}` or `{tool}` text, and never `facts.env.*` or
    /// screen text). The Karen who reaches past the hook API and types it
    /// herself, with the owner's permission in writing.
    RuntimeCommand {
        command: String,
        #[serde(default)]
        arguments: Option<String>,
    },
}

impl ActionDef {
    /// Every text this action renders, whatever shape it delivers them in:
    /// the reasons a gate says aloud and the arguments a `runtime_command`
    /// types. For validation that reads what a gate will put into the world.
    #[must_use]
    pub fn reasons(&self) -> Vec<&str> {
        match self {
            Self::Allow => Vec::new(),
            Self::RuntimeCommand { arguments, .. } => arguments.as_deref().into_iter().collect(),
            Self::Deny { reason }
            | Self::Ask { reason }
            | Self::Notify { reason }
            | Self::Modify { reason, .. } => vec![reason],
            Self::DenyOnceThenWarn {
                first_reason,
                second_reason,
            } => vec![first_reason, second_reason],
        }
    }
}

impl ActionDef {
    /// Every `{facts.<path>}` placeholder in the texts this action renders, as
    /// the path after `facts.`. A fact only a reason or an argument names is
    /// still a fact the gate reads, so it is collected like one a matcher
    /// names, and `karen validate` checks its spelling the same way.
    #[must_use]
    pub fn templated_facts(&self) -> Vec<&str> {
        let mut paths = Vec::new();
        for text in self.reasons() {
            let mut rest = text;
            while let Some(open) = rest.find("{facts.") {
                let after = &rest[open + "{facts.".len()..];
                let Some(close) = after.find('}') else {
                    break;
                };
                paths.push(&after[..close]);
                rest = &after[close + 1..];
            }
        }
        paths
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSpec {
    #[serde(default)]
    pub track: Track,
    pub key: String,
    #[serde(default = "default_max_strikes")]
    pub max_strikes: u32,
    /// Forget this key's strikes when the session compacts. A nudge that taught
    /// something the compaction then dropped gets to nudge once more.
    #[serde(default)]
    pub reset_on_compact: bool,
    /// The payload field a fired gate watches (`facts.context_ratio`), when
    /// the spec is a latch: the gate fires once, then stays quiet until the
    /// field falls back below [`Self::rearm_below`]. The fact is collected the
    /// same way a `field_*` condition's is, so a latch on `facts.X` reads it.
    #[serde(default)]
    pub field: Option<String>,
    /// The line the latch's `field` must fall back below before the gate may
    /// fire again (the ratio after a compaction, not the one that tripped it).
    #[serde(default)]
    pub rearm_below: Option<f64>,
    /// With `track: "input"`, the fields that make two calls "the same input":
    /// each path resolves the way a `field_*` condition's does (the tool input
    /// first, then the payload; `.` walks nested keys). Unset, a shell call
    /// counts without its `description` and any other tool by its whole input.
    /// Read off the request as it arrived: it collects no fact.
    #[serde(default)]
    pub key_fields: Option<Vec<String>>,
}

impl StateSpec {
    /// The latch this spec arms, when it is one: the `field` a fired gate
    /// watches, and the `rearm_below` line the field must fall back below
    /// before the gate may fire again. Both or neither, on any action: a
    /// latch without a line never re-arms, one without a field watches
    /// nothing, and a strike counter is not a latch at all.
    #[must_use]
    pub fn latch(&self) -> Option<(&str, f64)> {
        Some((self.field.as_deref()?, self.rearm_below?))
    }
}

/// What a strike count belongs to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Track {
    /// One count per session: the nudge teaches once, then warns.
    #[default]
    Session,
    /// One count per distinct tool input: each new command is held once, and
    /// the same command retried passes with the warning. A shell command's
    /// free-text `description` is not part of the input: the same command in
    /// other words is the same command. Every field that changes what runs
    /// (`timeout`, `workdir`) is, and so is a `description` that arrives
    /// without a `command`, because there it is the payload. A gate that
    /// knows better says so with `key_fields`.
    Input,
}

const fn default_max_strikes() -> u32 {
    2
}

/// A regex compiled on first use. Compiling every pattern of every gate cost
/// about 55 ms per hook call with 63 gates, almost all of a call; most calls
/// evaluate a handful. [`Gate::validate`] compiles them all when the policy
/// files change. The Karen who reads the clause when it applies to you.
#[derive(Debug, Clone)]
struct LazyRegex {
    pattern: String,
    compiled: OnceLock<Option<Regex>>,
}

impl LazyRegex {
    fn new(pattern: String) -> Self {
        Self {
            pattern,
            compiled: OnceLock::new(),
        }
    }

    /// An invalid pattern never matches; validation reports it.
    fn is_match(&self, text: &str) -> bool {
        self.compiled
            .get_or_init(|| Regex::new(&self.pattern).ok())
            .as_ref()
            .is_some_and(|re| re.is_match(text))
    }

    fn validate(&self) -> Result<(), regex::Error> {
        let re = Regex::new(&self.pattern)?;
        let _ = self.compiled.set(Some(re));
        Ok(())
    }
}

/// A gate ready to evaluate; its regexes compile on first use. The Karen who does her reading before the meeting, and expects the same of you.
#[derive(Debug, Clone)]
pub struct Gate {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub event: String,
    pub tool: Option<String>,
    pub priority: i32,
    pub enabled: bool,
    pub tags: Vec<String>,
    pub matcher: Matcher,
    pub action: ActionDef,
    pub state: Option<StateSpec>,
    pub locked: bool,
    pub replaces: bool,
    /// Where the gate was defined: its layer and file (`builtin` for the
    /// gates compiled into Karen). The loader fills these in.
    pub layer: Layer,
    pub source: String,
    tool_regex: Option<LazyRegex>,
}

#[derive(Debug, Clone, Default)]
pub struct Matcher {
    all: Vec<Condition>,
    any: Vec<Condition>,
    none: Vec<Condition>,
}

#[derive(Debug, Clone)]
enum Condition {
    CommandRegex(LazyRegex),
    CommandContains(String),
    FileRegex(LazyRegex),
    PathRegex(LazyRegex),
    PromptRegex(LazyRegex),
    ContentRegex(LazyRegex),
    ToolRegex(LazyRegex),
    FieldEquals(String, serde_json::Value),
    FieldRegex(String, LazyRegex),
    FieldJsonRegex(String, LazyRegex),
    FieldGte(String, f64),
    FieldLte(String, f64),
    FileOlderThan(String, u64),
    Always,
}

impl Condition {
    fn regex(&self) -> Option<&LazyRegex> {
        match self {
            Self::CommandRegex(re)
            | Self::FileRegex(re)
            | Self::PathRegex(re)
            | Self::PromptRegex(re)
            | Self::ContentRegex(re)
            | Self::ToolRegex(re)
            | Self::FieldRegex(_, re)
            | Self::FieldJsonRegex(_, re) => Some(re),
            Self::CommandContains(_)
            | Self::FieldEquals(..)
            | Self::FieldGte(..)
            | Self::FieldLte(..)
            | Self::FileOlderThan(..)
            | Self::Always => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("invalid regex in gate `{gate}`: {source}")]
    Regex {
        gate: String,
        #[source]
        source: regex::Error,
    },
    #[error("unknown event `{0}` in gate `{1}`")]
    UnknownEvent(String, String),
    #[error("gate `{gate}` has unusable state.key_fields: {reason}")]
    KeyFields { gate: String, reason: &'static str },
    #[error("gate `{gate}` has an unusable runtime_command: {reason}")]
    RuntimeCommand { gate: String, reason: &'static str },
}

impl Gate {
    /// Build a gate from its definition and check its event. Regexes compile on
    /// first use; [`Gate::validate`] compiles every one of them.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::UnknownEvent`] when the gate names an event
    /// she does not recognize, and [`CompileError::KeyFields`] when its
    /// `state.key_fields` is empty or names a blank path: a list that keys on
    /// nothing would call every input the same one. A `runtime_command` with a
    /// blank command is refused as [`CompileError::RuntimeCommand`]: there is
    /// nothing to type.
    pub fn compile(def: GateDef) -> Result<Self, CompileError> {
        validate_event(&def.event)
            .map_err(|()| CompileError::UnknownEvent(def.event.clone(), def.id.clone()))?;
        if matches!(&def.action, ActionDef::RuntimeCommand { command, .. } if command.trim().is_empty())
        {
            return Err(CompileError::RuntimeCommand {
                gate: def.id.clone(),
                reason: "the command is blank",
            });
        }
        if let Some(fields) = def.state.as_ref().and_then(|s| s.key_fields.as_ref()) {
            let reason = if fields.is_empty() {
                Some("the list is empty")
            } else if fields.iter().any(|f| f.trim().is_empty()) {
                Some("a path is blank")
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(CompileError::KeyFields {
                    gate: def.id.clone(),
                    reason,
                });
            }
        }

        let condition = |c: ConditionDef| match c {
            ConditionDef::CommandRegex { pattern } => {
                Condition::CommandRegex(LazyRegex::new(pattern))
            }
            ConditionDef::CommandContains { value } => Condition::CommandContains(value),
            ConditionDef::FileRegex { pattern } => Condition::FileRegex(LazyRegex::new(pattern)),
            ConditionDef::PathRegex { pattern } => Condition::PathRegex(LazyRegex::new(pattern)),
            ConditionDef::PromptRegex { pattern } => {
                Condition::PromptRegex(LazyRegex::new(pattern))
            }
            ConditionDef::ContentRegex { pattern } => {
                Condition::ContentRegex(LazyRegex::new(pattern))
            }
            ConditionDef::ToolRegex { pattern } => Condition::ToolRegex(LazyRegex::new(pattern)),
            ConditionDef::FieldEquals { field, value } => Condition::FieldEquals(field, value),
            ConditionDef::FieldRegex { field, pattern } => {
                Condition::FieldRegex(field, LazyRegex::new(pattern))
            }
            ConditionDef::FieldJsonRegex { field, pattern } => {
                Condition::FieldJsonRegex(field, LazyRegex::new(pattern))
            }
            ConditionDef::FieldGte { field, value } => Condition::FieldGte(field, value),
            ConditionDef::FieldLte { field, value } => Condition::FieldLte(field, value),
            ConditionDef::FileOlderThan { path, seconds } => {
                Condition::FileOlderThan(path, seconds)
            }
            ConditionDef::Always => Condition::Always,
        };

        let matcher = Matcher {
            all: def.r#match.all.into_iter().map(condition).collect(),
            any: def.r#match.any.into_iter().map(condition).collect(),
            none: def.r#match.none.into_iter().map(condition).collect(),
        };

        Ok(Self {
            id: def.id,
            name: def.name,
            description: def.description,
            event: def.event,
            tool_regex: def.tool.clone().map(LazyRegex::new),
            tool: def.tool,
            priority: def.priority,
            enabled: def.enabled,
            tags: def.tags,
            matcher,
            action: def.action,
            state: def.state,
            locked: def.locked,
            replaces: def.replaces,
            layer: Layer::Project,
            source: String::new(),
        })
    }

    /// Compile every pattern now: the `tool` filter and every match pattern.
    /// A `tool` filter matches an exact tool name too, but one that does not
    /// compile as a regex is refused here all the same, because a typo there
    /// leaves a gate that silently never applies to the tool it was written
    /// for.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::Regex`] for the first pattern, the `tool` filter
    /// included, that is not a valid regex.
    pub fn validate(&self) -> Result<(), CompileError> {
        self.tool_regex
            .iter()
            .chain(self.matcher.conditions().filter_map(Condition::regex))
            .try_for_each(LazyRegex::validate)
            .map_err(|source| CompileError::Regex {
                gate: self.id.clone(),
                source,
            })
    }

    /// Every pattern [`Gate::validate`] compiles, in a fixed order.
    pub fn patterns(&self) -> impl Iterator<Item = &str> {
        self.tool_regex
            .iter()
            .chain(self.matcher.conditions().filter_map(Condition::regex))
            .map(|re| re.pattern.as_str())
    }

    #[must_use]
    pub fn applies_to(&self, event: &Event, tool: Option<&str>) -> bool {
        self.enabled
            && event_name(event).eq_ignore_ascii_case(&self.event)
            && self.tool.as_deref().is_none_or(|t| {
                tool.is_some_and(|actual| {
                    actual.eq_ignore_ascii_case(t)
                        || self
                            .tool_regex
                            .as_ref()
                            .is_some_and(|re| re.is_match(actual))
                })
            })
    }
}

impl Matcher {
    fn conditions(&self) -> impl Iterator<Item = &Condition> {
        self.all.iter().chain(&self.any).chain(&self.none)
    }

    /// Every payload field a `field_*` condition reads.
    pub fn fields(&self) -> impl Iterator<Item = &str> {
        self.conditions().filter_map(|c| match c {
            Condition::FieldEquals(field, _)
            | Condition::FieldRegex(field, _)
            | Condition::FieldJsonRegex(field, _)
            | Condition::FieldGte(field, _)
            | Condition::FieldLte(field, _) => Some(field.as_str()),
            _ => None,
        })
    }

    /// Short-circuits: `any` and `none` are read only while `all` holds, so a
    /// gate that cannot match compiles nothing past its first failed condition.
    #[must_use]
    pub fn matches(&self, req: &crate::types::EngineRequest) -> bool {
        self.all.iter().all(|c| c.matches(req))
            && (self.any.is_empty() || self.any.iter().any(|c| c.matches(req)))
            && !self.none.iter().any(|c| c.matches(req))
    }
}

/// `tool_input.<path>`, else the top-level payload `<path>`; `.` separates nested keys.
pub(crate) fn lookup<'a>(
    req: &'a crate::types::EngineRequest,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let walk = |root: &'a serde_json::Value| path.split('.').try_fold(root, |v, k| v.get(k));
    req.tool_input
        .as_ref()
        .and_then(walk)
        .or_else(|| walk(&req.raw))
}

impl Condition {
    fn matches(&self, req: &crate::types::EngineRequest) -> bool {
        match self {
            Self::CommandRegex(re) => req.command().is_some_and(|c| re.is_match(c)),
            Self::CommandContains(s) => req.command().is_some_and(|c| c.contains(s)),
            Self::FileRegex(re) => req.file_path().is_some_and(|p| re.is_match(p)),
            Self::PathRegex(re) => req.file_path().is_some_and(|p| re.is_match(p)),
            Self::PromptRegex(re) => req
                .prompt
                .as_deref()
                .or(req.user_message.as_deref())
                .is_some_and(|p| re.is_match(p)),
            Self::ContentRegex(re) => {
                let Some(input) = req.tool_input.as_ref() else {
                    return false;
                };
                ["content", "new_string", "newString"]
                    .iter()
                    .filter_map(|k| input.get(k).and_then(serde_json::Value::as_str))
                    .any(|s| re.is_match(s))
            }
            Self::ToolRegex(re) => req.tool_name.as_deref().is_some_and(|t| re.is_match(t)),
            // tool_input first; payloads without the field fall back to the
            // top-level hook fields (Stop's `stop_hook_active`, `agent_type`,
            // ...). The Karen who checks the drawer, then the desk it sits in.
            // A dotted path (`tool_response.stdout`) walks nested objects.
            Self::FieldEquals(field, value) => {
                lookup(req, field).is_some_and(|actual| actual == value)
            }
            // Same lookup, a pattern instead of an exact value: string fields
            // only. Stop's `last_assistant_message` is the obvious customer —
            // she reads the closing statement before she lets you leave; and
            // PostToolUse's `tool_response.stdout`, what you just showed everyone.
            Self::FieldRegex(field, re) => lookup(req, field)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|s| re.is_match(s)),
            // Any value: a string as itself, anything else (Stop's
            // `background_tasks` array, an object) as its compact JSON text.
            Self::FieldJsonRegex(field, re) => {
                lookup(req, field).is_some_and(|v| match v.as_str() {
                    Some(s) => re.is_match(s),
                    None => re.is_match(&v.to_string()),
                })
            }
            // A number compared, not read: the field is absent or not numeric
            // and there is nothing to compare, so the condition never matches.
            // Absence is not a number, and she does not round one up to zero.
            Self::FieldGte(field, value) => lookup(req, field)
                .and_then(serde_json::Value::as_f64)
                .is_some_and(|number| number >= *value),
            Self::FieldLte(field, value) => lookup(req, field)
                .and_then(serde_json::Value::as_f64)
                .is_some_and(|number| number <= *value),
            // A timer without a clock: the file's age is the clock, so a gate can say
            // "due again" by watching a marker that the finished chore touches. The
            // Karen who checks when you last did it, not whether you promised to.
            Self::FileOlderThan(path, seconds) => {
                file_older_than(path, req.cwd.as_deref(), *seconds)
            }
            Self::Always => true,
        }
    }
}

fn file_older_than(path: &str, cwd: Option<&str>, seconds: u64) -> bool {
    let resolved = if path.starts_with("~/") {
        crate::home::expand(path)
    } else {
        let p = std::path::PathBuf::from(path);
        match cwd {
            Some(dir) if p.is_relative() => std::path::Path::new(dir).join(p),
            _ => p,
        }
    };
    let Ok(modified) = std::fs::metadata(&resolved).and_then(|m| m.modified()) else {
        return true;
    };
    std::time::SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age.as_secs() > seconds)
}

#[must_use]
pub fn event_name(event: &Event) -> String {
    match event {
        Event::SessionStart => "SessionStart".into(),
        Event::SessionEnd => "SessionEnd".into(),
        Event::UserPromptSubmit => "UserPromptSubmit".into(),
        Event::PreToolUse => "PreToolUse".into(),
        Event::PostToolUse => "PostToolUse".into(),
        Event::PreCompact => "PreCompact".into(),
        Event::PostCompact => "PostCompact".into(),
        Event::SubagentStart => "SubagentStart".into(),
        Event::SubagentStop => "SubagentStop".into(),
        Event::Stop => "Stop".into(),
        Event::Notification => "Notification".into(),
        Event::ProviderError => "ProviderError".into(),
        Event::Virtual(v) => format!("Virtual{v:?}"),
    }
}

/// Every event a gate may name: the one list `karen validate`, `karen schema`
/// and the README's events table all answer to.
pub const EVENT_NAMES: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "Notification",
    "ProviderError",
];

fn validate_event(event: &str) -> Result<(), ()> {
    if EVENT_NAMES.iter().any(|k| k.eq_ignore_ascii_case(event)) {
        Ok(())
    } else {
        Err(())
    }
}

impl fmt::Display for Gate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({} @ {})", self.id, self.event, self.priority)
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionDef, EVENT_NAMES};

    fn deny(reason: &str) -> ActionDef {
        ActionDef::Deny {
            reason: reason.into(),
        }
    }

    #[test]
    fn templated_facts_are_the_paths_after_facts_in_every_text_an_action_renders() {
        assert_eq!(
            deny("{facts.runtime}, then {facts.provider_error.message} twice: {facts.runtime}")
                .templated_facts(),
            ["runtime", "provider_error.message", "runtime"]
        );
        let twice = ActionDef::DenyOnceThenWarn {
            first_reason: "a {facts.context_ratio}".into(),
            second_reason: "b {facts.session_id}".into(),
        };
        assert_eq!(twice.templated_facts(), ["context_ratio", "session_id"]);
        let typed = ActionDef::RuntimeCommand {
            command: "/compact".into(),
            arguments: Some("for {facts.session_id}".into()),
        };
        assert_eq!(typed.templated_facts(), ["session_id"]);
    }

    #[test]
    fn templated_facts_ignore_what_is_not_a_closed_fact_placeholder() {
        let none = Vec::<&str>::new();
        assert_eq!(
            deny("no placeholders, {gate} and {tool}").templated_facts(),
            none
        );
        assert_eq!(deny("an unclosed {facts.runtime").templated_facts(), none);
        assert_eq!(ActionDef::Allow.templated_facts(), none);
        let bare = ActionDef::RuntimeCommand {
            command: "/compact".into(),
            arguments: None,
        };
        assert_eq!(bare.templated_facts(), none);
    }

    /// The README's events table, as the event names in its first column.
    fn documented_events() -> Vec<String> {
        include_str!("../../README.md")
            .lines()
            .skip_while(|line| *line != "### Events")
            .skip(1)
            .take_while(|line| !line.starts_with("### "))
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|row| row.split('`').next())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_readmes_events_table_names_exactly_the_events_she_accepts() {
        // A table that drifts from the code is a rumour with a border.
        let mut documented = documented_events();
        documented.sort();
        let mut known: Vec<String> = EVENT_NAMES.iter().map(|name| (*name).to_string()).collect();
        known.sort();
        assert_eq!(documented, known);
    }
}
