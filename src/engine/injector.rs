//! Rule and context injection. The Karen who injects, and never forgets a demand.

use crate::policy::PolicySet;
use crate::state::StateStore;
use crate::types::{Injection, InjectionKind};

const INJECTED_KEY: &str = "__injected_rules";
const SEPARATOR: &str = "\n\n---\n\n";

/// The harness caps injected context at 10,000 chars. She chunks rules below that so nothing gets truncated. The Karen who reads the fine print.
pub const CHUNK_LIMIT: usize = 9_800;

/// Upper bound used when clearing per-part injection markers. The Karen who bounds, because "at some point" is not a number and she refuses to accept it.
const MAX_PARTS: usize = 64;

/// Build the always-on rule payload, or `None` if already injected. The Karen who says it once, in writing, and points at the record when you claim you were never told.
///
/// With `part`, only that 1-based chunk is returned and tracked under its own
/// state key so sibling hook processes can each deliver one chunk. Without a
/// part, the complete payload is returned.
///
/// Rules are re-injected after compaction because context is rebuilt. The Karen who refuses to let her demand for the manager be forgotten.
pub fn rule_injection(
    policies: &PolicySet,
    state: &dyn StateStore,
    session: &str,
    force: bool,
    part: Option<usize>,
) -> Option<Injection> {
    if policies.rules.is_empty() {
        return None;
    }

    let key = state_key(part);
    if !force && state.get(session, &key).is_some() {
        return None;
    }

    let parts = rule_parts(policies);
    let content = match part {
        Some(1..) => parts.get(part? - 1)?.clone(),
        Some(0) => return None,
        None => parts.join(SEPARATOR),
    };

    state.set(session, &key, serde_json::Value::Bool(true));

    Some(Injection {
        content,
        kind: InjectionKind::Rule,
        once_per_session: true,
        source: Some(policies.root.display().to_string()),
    })
}

/// Pack rules into chunks below [`CHUNK_LIMIT`], preferring rule boundaries. The Karen who packs it tight, like a suitcase she intends to inspect.
#[must_use]
pub fn rule_parts(policies: &PolicySet) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();

    for rule in &policies.rules {
        let text = rule.content.trim_end();
        if text.is_empty() {
            continue;
        }
        if current.is_empty() {
            append_chunked(&mut parts, &mut current, text);
        } else if current.len() + SEPARATOR.len() + text.len() <= CHUNK_LIMIT {
            current.push_str(SEPARATOR);
            current.push_str(text);
        } else {
            parts.push(std::mem::take(&mut current));
            append_chunked(&mut parts, &mut current, text);
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Append `text` to `current`, emitting over-long text as multiple parts. The Karen who splits the check by item, with math she'll show you.
fn append_chunked(parts: &mut Vec<String>, current: &mut String, text: &str) {
    if text.len() <= CHUNK_LIMIT {
        current.push_str(text);
        return;
    }

    let mut rest = text;
    while rest.len() > CHUNK_LIMIT {
        let mut cut = CHUNK_LIMIT;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        parts.push(rest[..cut].to_string());
        rest = &rest[cut..];
    }
    current.push_str(rest);
}

fn state_key(part: Option<usize>) -> String {
    part.map_or_else(
        || INJECTED_KEY.to_string(),
        |n| format!("{INJECTED_KEY}_part_{n}"),
    )
}

/// Clear the injection markers so the next turn re-sends rules. The Karen who clears the board for round two.
pub fn invalidate(state: &dyn StateStore, session: &str) {
    state.remove(session, INJECTED_KEY);
    for part in 1..=MAX_PARTS {
        state.remove(session, &format!("{INJECTED_KEY}_part_{part}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Config, PolicySet, Rule};
    use crate::state::MemoryStore;
    use std::path::PathBuf;

    fn policies(contents: &[&str]) -> PolicySet {
        PolicySet {
            gates: Vec::new(),
            rules: contents
                .iter()
                .enumerate()
                .map(|(i, content)| Rule {
                    id: format!("rule-{i}"),
                    path: PathBuf::from(format!("rule-{i}.md")),
                    content: (*content).to_string(),
                })
                .collect(),
            config: Config::default(),
            root: PathBuf::from("policies"),
        }
    }

    #[test]
    fn small_rule_set_is_one_part() {
        let set = policies(&["alpha", "beta"]);
        let parts = rule_parts(&set);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], "alpha\n\n---\n\nbeta");
    }

    #[test]
    fn parts_respect_the_chunk_limit() {
        let rule_a = "a".repeat(5_000);
        let rule_b = "b".repeat(5_000);
        let set = policies(&[&rule_a, &rule_b]);
        let parts = rule_parts(&set);
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.len() <= CHUNK_LIMIT));
    }

    #[test]
    fn oversized_rule_is_split_on_char_boundaries() {
        let rule = "é".repeat(6_000);
        let set = policies(&[&rule]);
        let parts = rule_parts(&set);
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|p| p.len() <= CHUNK_LIMIT));
        assert_eq!(parts.concat(), rule);
    }

    #[test]
    fn each_part_is_injected_once_per_session() {
        let set = policies(&["alpha"]);
        let state = MemoryStore::new();

        assert!(rule_injection(&set, &state, "s1", false, Some(1)).is_some());
        assert!(rule_injection(&set, &state, "s1", false, Some(1)).is_none());
        assert!(rule_injection(&set, &state, "s1", false, Some(2)).is_none());
        assert!(rule_injection(&set, &state, "s2", false, Some(1)).is_some());
    }

    #[test]
    fn invalidate_clears_every_part() {
        let set = policies(&["alpha"]);
        let state = MemoryStore::new();

        assert!(rule_injection(&set, &state, "s1", false, Some(1)).is_some());
        invalidate(&state, "s1");
        assert!(rule_injection(&set, &state, "s1", false, Some(1)).is_some());
    }
}
