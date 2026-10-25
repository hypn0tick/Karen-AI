//! The vocabulary she understands. Events, actions, injections — all defined here. The Karen who speaks your language.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The hook event that brought you here. The Karen who classifies, and will be asking how you got in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Event {
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    PreCompact,
    PostCompact,
    SubagentStart,
    SubagentStop,
    Stop,
    Notification,
    Virtual(VirtualEvent),
}

/// Events she cooks up herself — the housekeeping kind. The Karen who invents her own holidays, and expects you to observe them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VirtualEvent {
    SessionResume,
    RulesReload,
    ContextPressure,
}

/// The request she evaluates — boiled down from whatever dialect the runtime speaks. The Karen who normalizes, because runtimes can't spell "consistent" without her.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRequest {
    pub event: Event,
    pub session_id: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_response: Option<serde_json::Value>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub user_message: Option<String>,
    #[serde(default)]
    pub metadata: RequestMetadata,
    /// Rule-injection part the hook asked for (1-based). The Karen who chunks — one piece at a time, like a docent who suspects you of touching things, and has already notified the front desk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<usize>,
    /// Raw payload she keeps for runtime-specific policies. The Karen who hoards context, because discarding evidence is what guilty people do.
    #[serde(default)]
    pub raw: serde_json::Value,
}

impl EngineRequest {
    #[must_use]
    pub fn command(&self) -> Option<&str> {
        self.tool_input
            .as_ref()
            .and_then(|v| v.get("command"))
            .and_then(serde_json::Value::as_str)
    }

    #[must_use]
    pub fn file_path(&self) -> Option<&str> {
        let input = self.tool_input.as_ref()?;
        input
            .get("file_path")
            .or_else(|| input.get("filePath"))
            .or_else(|| input.get("path"))
            .and_then(serde_json::Value::as_str)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestMetadata {
    #[serde(default)]
    pub is_subagent: bool,
    #[serde(default)]
    pub is_first_turn: bool,
    #[serde(default)]
    pub is_post_compact: bool,
}

/// The verdict she delivers. The Karen who decides — appeals accepted during regular business hours, which are also the hours she denies appeals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
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
        modified_input: Option<serde_json::Value>,
    },
}

impl Action {
    #[must_use]
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn ask(reason: impl Into<String>) -> Self {
        Self::Ask {
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn modify(reason: impl Into<String>) -> Self {
        Self::Modify {
            reason: reason.into(),
            modified_input: None,
        }
    }

    #[must_use]
    pub fn with_modified_input(mut self, input: Option<serde_json::Value>) -> Self {
        if let Self::Modify { modified_input, .. } = &mut self {
            *modified_input = input;
        }
        self
    }

    #[must_use]
    pub fn priority(&self) -> u8 {
        match self {
            Self::Deny { .. } => 3,
            Self::Modify { .. } => 2,
            Self::Ask { .. } => 1,
            Self::Allow => 0,
        }
    }
}

/// The context fragment she injects. The Karen who injects, because you were going to skip the reading otherwise — and she has highlighted the parts you'll claim you missed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Injection {
    pub content: String,
    #[serde(default)]
    pub kind: InjectionKind,
    #[serde(default)]
    pub once_per_session: bool,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionKind {
    #[default]
    Rule,
    Skill,
    Redirect,
    Notice,
}

/// The response she hands back. The Karen who replies to everything, in writing, promptly — because "you never told me" deserves a timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineResponse {
    pub action: Action,
    #[serde(default)]
    pub injections: Vec<Injection>,
    #[serde(default)]
    pub modified_input: Option<serde_json::Value>,
    #[serde(default)]
    pub fired_policies: Vec<String>,
}

impl EngineResponse {
    #[must_use]
    pub fn allow() -> Self {
        Self {
            action: Action::Allow,
            injections: Vec::new(),
            modified_input: None,
            fired_policies: Vec::new(),
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allow => write!(f, "allow"),
            Self::Deny { .. } => write!(f, "deny"),
            Self::Ask { .. } => write!(f, "ask"),
            Self::Modify { .. } => write!(f, "modify"),
        }
    }
}

/// Runtime identity so policies can specialize. The Karen who knows who's talking, and has opinions about all of them, filed alphabetically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterType {
    #[default]
    #[serde(alias = "claude")]
    Claude,
    #[serde(alias = "claude-classic")]
    ClaudeClassic,
    Codex,
    OpenCodeV1,
    OpenCodeV2,
}
