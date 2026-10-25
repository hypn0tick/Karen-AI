//! Claude Code "Classic" hooks: JSON on stdin, JSON on stdout. The Karen who speaks Classic and is *very* proud of herself.

use super::{AdapterError, RuntimeAdapter};
use crate::types::{
    Action, AdapterType, EngineRequest, EngineResponse, Event, InjectionKind, RequestMetadata,
};
use serde_json::{Value, json};

#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeClassic;

impl RuntimeAdapter for ClaudeClassic {
    fn kind(&self) -> AdapterType {
        AdapterType::ClaudeClassic
    }

    fn normalize(&self, raw: Value) -> Result<EngineRequest, AdapterError> {
        let obj = raw.as_object().ok_or(AdapterError::NotObject)?;

        let hook_event = obj
            .get("hook_event_name")
            .and_then(Value::as_str)
            .unwrap_or("PreToolUse");

        let event = match hook_event {
            "SessionStart" => Event::SessionStart,
            "SessionEnd" => Event::SessionEnd,
            "UserPromptSubmit" => Event::UserPromptSubmit,
            "PreToolUse" => Event::PreToolUse,
            "PostToolUse" => Event::PostToolUse,
            "PreCompact" => Event::PreCompact,
            "PostCompact" => Event::PostCompact,
            "SubagentStart" => Event::SubagentStart,
            "SubagentStop" => Event::SubagentStop,
            "Stop" => Event::Stop,
            "Notification" => Event::Notification,
            other => return Err(AdapterError::UnknownEvent(other.to_string())),
        };

        let session_id = obj
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let prompt = obj
            .get("prompt")
            .and_then(Value::as_str)
            .map(str::to_string);

        Ok(EngineRequest {
            event,
            session_id,
            agent_id: obj
                .get("agent_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            cwd: obj.get("cwd").and_then(Value::as_str).map(str::to_string),
            tool_name: obj
                .get("tool_name")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool_input: obj.get("tool_input").cloned(),
            tool_response: obj.get("tool_response").cloned(),
            prompt: prompt.clone(),
            user_message: prompt,
            metadata: RequestMetadata {
                is_subagent: obj.get("agent_id").is_some() || obj.contains_key("agent_type"),
                ..RequestMetadata::default()
            },
            part: obj
                .get("part")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok()),
            raw,
        })
    }

    fn denormalize(&self, request: &EngineRequest, response: EngineResponse) -> Value {
        let context = response
            .injections
            .iter()
            .filter(|i| matches!(i.kind, InjectionKind::Rule | InjectionKind::Redirect))
            .map(|i| i.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let notices = response
            .injections
            .iter()
            .filter(|i| matches!(i.kind, InjectionKind::Notice | InjectionKind::Skill))
            .map(|i| i.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if matches!(response.action, Action::Allow) {
            // Allow is the protocol's polite silence — unless she has rules to
            // deliver or a warning to attach. The Karen who speaks up only
            // when she has something worth saying, which is more often than
            // you'd hope.
            let mut out = if matches!(request.event, Event::SessionStart | Event::UserPromptSubmit)
                && !context.is_empty()
            {
                let event_name = if matches!(request.event, Event::SessionStart) {
                    "SessionStart"
                } else {
                    "UserPromptSubmit"
                };
                json!({
                    "hookSpecificOutput": {
                        "hookEventName": event_name,
                        "additionalContext": context,
                    }
                })
            } else {
                json!({})
            };

            // A warned retry is allowed, but never silently. She attaches the
            // gate's own closing words plus a nudge, because pretending not to
            // notice is how habits form. The Karen who lets it slide while
            // making it abundantly clear she saw the whole thing — and that the
            // manager's number is already dialed, she just hasn't pressed call.
            // One more retry and she presses it.
            if !notices.is_empty() {
                let message = format!(
                    "{notices} Next time, prefer the right tool first — this fallback has been noted."
                );
                if let Some(object) = out.as_object_mut() {
                    object.insert("systemMessage".into(), json!(message));
                }
            }
            return out;
        }

        let mut out = match (request.event.clone(), &response.action) {
            (Event::PreToolUse, Action::Deny { reason }) => json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            }),
            (Event::PreToolUse, Action::Ask { reason }) => json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "ask",
                    "permissionDecisionReason": reason,
                }
            }),
            (
                Event::PreToolUse,
                Action::Modify {
                    reason,
                    modified_input,
                },
            ) => json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "ask",
                    "permissionDecisionReason": reason,
                },
                "modifiedInput": modified_input,
            }),
            (Event::UserPromptSubmit | Event::PostToolUse, Action::Deny { reason }) => json!({
                "decision": "block",
                "reason": reason,
            }),
            (Event::UserPromptSubmit, _) if !context.is_empty() => json!({
                "hookSpecificOutput": {
                    "hookEventName": "UserPromptSubmit",
                    "additionalContext": context,
                }
            }),
            (Event::SessionStart, _) if !context.is_empty() => json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": context,
                }
            }),
            (_, Action::Deny { reason }) => json!({ "decision": "block", "reason": reason }),
            _ => json!({}),
        };

        // The verdict blocks the call AND the coaching surfaces in the same
        // breath, because a first strike that only lectures without denying
        // is a suggestion wearing a badge. The Karen who denies, explains,
        // and files a copy with the manager — who appreciates a paper trail.
        if !notices.is_empty() {
            if let Some(object) = out.as_object_mut() {
                object.insert("systemMessage".into(), json!(notices));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> EngineRequest {
        ClaudeClassic
            .normalize(json!({
                "hook_event_name": "PreToolUse",
                "session_id": "abc",
                "cwd": "/tmp",
                "tool_name": "Bash",
                "tool_input": { "command": "git commit -m 'x'" }
            }))
            .expect("normalize")
    }

    #[test]
    fn normalizes_claude_payload() {
        let req = request();
        assert_eq!(req.event, Event::PreToolUse);
        assert_eq!(req.session_id, "abc");
        assert_eq!(req.command(), Some("git commit -m 'x'"));
    }

    #[test]
    fn denies_with_claude_shape() {
        let req = request();
        let out = ClaudeClassic.denormalize(
            &req,
            EngineResponse {
                action: Action::deny("no"),
                injections: vec![],
                modified_input: None,
                fired_policies: vec![],
            },
        );
        assert_eq!(
            out["hookSpecificOutput"]["permissionDecision"],
            Value::String("deny".into())
        );
    }

    #[test]
    fn allow_is_empty_object() {
        let req = request();
        let out = ClaudeClassic.denormalize(&req, EngineResponse::allow());
        assert_eq!(out, json!({}));
    }
}
