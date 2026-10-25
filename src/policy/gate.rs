//! Gates: the JSON/JSONC/JSON5 you write, the verdicts she delivers. Four answers: allow, deny, ask, modify. No maybe. The Karen who decides.

use crate::types::Event;
use regex::Regex;
use serde::Deserialize;
use std::fmt;

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
    /// Optional tool filter (`Bash`, `Edit`, `Write`, ...). The Karen who filters.
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSpec {
    #[serde(default = "default_track")]
    pub track: String,
    pub key: String,
    #[serde(default = "default_max_strikes")]
    pub max_strikes: u32,
}

fn default_track() -> String {
    "session".into()
}

const fn default_max_strikes() -> u32 {
    2
}

/// A gate with compiled regexes, ready to evaluate. The Karen who does her reading before the meeting, and expects the same of you.
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
}

#[derive(Debug, Clone, Default)]
pub struct Matcher {
    all: Vec<Condition>,
    any: Vec<Condition>,
    none: Vec<Condition>,
}

#[derive(Debug, Clone)]
enum Condition {
    CommandRegex(Regex),
    CommandContains(String),
    FileRegex(Regex),
    PathRegex(Regex),
    PromptRegex(Regex),
    ContentRegex(Regex),
    ToolRegex(Regex),
    FieldEquals(String, serde_json::Value),
    Always,
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
}

impl Gate {
    /// Compile a gate definition: validate the event and pre-compile every
    /// regex, so evaluation never pays the compile cost twice. The Karen
    /// who does the work once and bills you forever.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::UnknownEvent`] when the gate names an event
    /// she does not recognize, and [`CompileError::Regex`] when one of its
    /// match patterns is not a valid regex.
    pub fn compile(def: GateDef) -> Result<Self, CompileError> {
        validate_event(&def.event)
            .map_err(|()| CompileError::UnknownEvent(def.event.clone(), def.id.clone()))?;

        let compile = |c: ConditionDef| -> Result<Condition, CompileError> {
            let re = |pattern: &str| {
                Regex::new(pattern).map_err(|source| CompileError::Regex {
                    gate: def.id.clone(),
                    source,
                })
            };
            Ok(match c {
                ConditionDef::CommandRegex { pattern } => Condition::CommandRegex(re(&pattern)?),
                ConditionDef::CommandContains { value } => Condition::CommandContains(value),
                ConditionDef::FileRegex { pattern } => Condition::FileRegex(re(&pattern)?),
                ConditionDef::PathRegex { pattern } => Condition::PathRegex(re(&pattern)?),
                ConditionDef::PromptRegex { pattern } => Condition::PromptRegex(re(&pattern)?),
                ConditionDef::ContentRegex { pattern } => Condition::ContentRegex(re(&pattern)?),
                ConditionDef::ToolRegex { pattern } => Condition::ToolRegex(re(&pattern)?),
                ConditionDef::FieldEquals { field, value } => Condition::FieldEquals(field, value),
                ConditionDef::Always => Condition::Always,
            })
        };

        let matcher = Matcher {
            all: def
                .r#match
                .all
                .into_iter()
                .map(&compile)
                .collect::<Result<_, _>>()?,
            any: def
                .r#match
                .any
                .into_iter()
                .map(&compile)
                .collect::<Result<_, _>>()?,
            none: def
                .r#match
                .none
                .into_iter()
                .map(&compile)
                .collect::<Result<_, _>>()?,
        };

        Ok(Self {
            id: def.id,
            name: def.name,
            description: def.description,
            event: def.event,
            tool: def.tool,
            priority: def.priority,
            enabled: def.enabled,
            tags: def.tags,
            matcher,
            action: def.action,
            state: def.state,
        })
    }

    #[must_use]
    pub fn applies_to(&self, event: &Event, tool: Option<&str>) -> bool {
        self.enabled
            && event_name(event).eq_ignore_ascii_case(&self.event)
            && self.tool.as_deref().is_none_or(|t| {
                tool.is_some_and(|actual| {
                    actual.eq_ignore_ascii_case(t)
                        || regex::Regex::new(t).is_ok_and(|re| re.is_match(actual))
                })
            })
    }
}

impl Matcher {
    #[must_use]
    pub fn matches(&self, req: &crate::types::EngineRequest) -> bool {
        let all_ok = self.all.iter().all(|c| c.matches(req));
        let any_ok = self.any.is_empty() || self.any.iter().any(|c| c.matches(req));
        let none_ok = !self.none.iter().any(|c| c.matches(req));
        all_ok && any_ok && none_ok
    }
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
            Self::FieldEquals(field, value) => req
                .tool_input
                .as_ref()
                .and_then(|i| i.get(field))
                .is_some_and(|actual| actual == value),
            Self::Always => true,
        }
    }
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
        Event::Virtual(v) => format!("Virtual{v:?}"),
    }
}

fn validate_event(event: &str) -> Result<(), ()> {
    const KNOWN: &[&str] = &[
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
    ];
    if KNOWN.iter().any(|k| k.eq_ignore_ascii_case(event)) {
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
