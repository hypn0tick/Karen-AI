//! Live reload: she watches the law and swaps it mid-flight. The Karen
//! who changes her mind before you finish saving it.
//!
//! For in-process embedders — a runtime that links this crate and calls
//! [`Engine::dispatch`] keeps one engine alive and lets this watcher
//! keep it current. The CLI gets the same promise architecturally:
//! every hook invocation is a fresh process that reads the policy root
//! as it stands, so Ctrl+S is the only reload button it will ever need.
//! In here, she watches the button for you.

use crate::engine::Engine;
use crate::policy;
use notify::Watcher as _;
use std::path::PathBuf;
use std::sync::Arc;

/// Keeps the file watcher alive. Drop to stop watching — the Karen who
/// leaves when the shift ends, never a moment before.
pub struct WatchGuard {
    _watcher: notify::RecommendedWatcher,
}

impl std::fmt::Debug for WatchGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WatchGuard(the Karen who is currently watching)")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error(
        "hot reload is disabled in config (engine.hot_reload = false) — she is only as live as you allow"
    )]
    Disabled,
    #[error("watcher failed to start: {0}")]
    Watch(#[from] notify::Error),
}

impl Engine {
    /// Watch `root` and hot-swap the policies whenever it changes.
    ///
    /// # Errors
    ///
    /// Returns [`WatchError::Disabled`] when the loaded config turns
    /// hot reload off, and [`WatchError::Watch`] when the file watcher
    /// cannot start. A failed *reload* is not an error: she keeps the
    /// last good law on file and logs the offense — the Karen who never
    /// un-enforces working policy over a broken save.
    ///
    /// # Panics
    ///
    /// If the policy lock is poisoned during a reload attempt. The
    /// watcher keeps the last good law and moves on, but a poisoned
    /// cabinet is beyond even her filing system.
    pub fn watch_policies(&self, root: impl Into<PathBuf>) -> Result<WatchGuard, WatchError> {
        if !self.policies().config.engine.hot_reload {
            return Err(WatchError::Disabled);
        }
        let root: PathBuf = root.into();
        let shared = self.shared_policies();
        let watch_root = Arc::new(root.clone());
        let mut watcher =
            notify::recommended_watcher(move |_event: Result<notify::Event, notify::Error>| {
                // A save often lands as several events; a reload is a
                // full re-read of a small directory, so she simply
                // reloads once per event and calls it thorough.
                match policy::load(&watch_root) {
                    Ok(set) => {
                        *shared
                            .write()
                            .expect("policy lock poisoned — she does not freeze mid-ruling") = set;
                        tracing::info!(
                            "policies reloaded — the law changed and she noticed instantly"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "live reload failed; keeping the last good law on file: {e}"
                        );
                    }
                }
            })?;
        watcher.watch(&root, notify::RecursiveMode::Recursive)?;
        Ok(WatchGuard { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    // The Karen who proves live reload works, because "it should work"
    // is what other projects say right before it doesn't. Three proofs:
    // the law swaps mid-flight, the off switch refuses by name, and a
    // broken save never un-makes the standing law.
    use super::*;
    use crate::state::MemoryStore;
    use crate::types::{Action, EngineRequest, Event, RequestMetadata};

    fn request(command: &str) -> EngineRequest {
        EngineRequest {
            event: Event::PreToolUse,
            session_id: "watch-test".into(),
            agent_id: None,
            cwd: None,
            tool_name: Some("Bash".into()),
            tool_input: Some(serde_json::json!({ "command": command })),
            tool_response: None,
            prompt: None,
            user_message: None,
            metadata: RequestMetadata::default(),
            part: None,
            raw: serde_json::json!({}),
        }
    }

    fn minimal_root(dir: &std::path::Path, hot_reload: bool) -> std::path::PathBuf {
        let root = dir.join("policies");
        std::fs::create_dir_all(root.join("gates")).expect("gates dir");
        std::fs::write(
            root.join("config.toml"),
            format!("[engine]\nfail_open = true\nhot_reload = {hot_reload}\n"),
        )
        .expect("config");
        root
    }

    fn deny_gate(id: &str, needle: &str) -> String {
        format!(
            r#"{{ "id": "{id}", "event": "PreToolUse", "match": {{ "all": [ {{ "type": "command_contains", "value": "{needle}" }} ] }}, "action": {{ "type": "deny", "reason": "The law is the law." }} }}"#
        )
    }

    #[test]
    fn karen_hot_swaps_the_law_mid_flight() {
        // The proof the capability demanded: an engine, alive in
        // process, ruling from old law — then the file changes on
        // disk, and the NEXT dispatch denies with the new gate, with
        // no new engine constructed. She noticed the save before the
        // editor finished dimming the title bar.
        let dir = tempfile::tempdir().expect("temp root");
        let root = minimal_root(dir.path(), true);
        let set = crate::policy::load(&root).expect("initial load");
        let engine = Engine::new(set, Box::new(MemoryStore::new()));
        let _guard = engine
            .watch_policies(&root)
            .expect("watcher starts — the knob is on");

        // Baseline: the command sails through — nothing forbids it.
        assert!(
            matches!(
                engine.dispatch(&request("run the forbidden ritual")).action,
                Action::Allow
            ),
            "baseline must allow — nothing is law yet"
        );

        // The law changes on disk. She is not told; she is watching.
        std::fs::write(
            root.join("gates").join("new-law.json"),
            deny_gate("new-law", "forbidden"),
        )
        .expect("write new law");

        // Poll: the watcher delivers on its own schedule; the verdict
        // must arrive without anyone rebuilding the engine.
        let mut denied = false;
        for _ in 0..200 {
            let response = engine.dispatch(&request("run the forbidden ritual"));
            if matches!(response.action, Action::Deny { .. }) {
                assert!(
                    response.fired_policies.iter().any(|id| id == "new-law"),
                    "the deny must come from the new gate, not a mood: {:?}",
                    response.fired_policies
                );
                denied = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            denied,
            "she never picked up the new law — live reload failed"
        );
    }

    #[test]
    fn karen_respects_the_off_switch() {
        // hot_reload = false: the watcher refuses to start, by name.
        // A disabled capability that silently runs anyway is not a
        // configuration option; it's a surveillance feature.
        let dir = tempfile::tempdir().expect("temp root");
        let root = minimal_root(dir.path(), false);
        let set = crate::policy::load(&root).expect("initial load");
        let engine = Engine::new(set, Box::new(MemoryStore::new()));
        let result = engine.watch_policies(&root);
        assert!(
            matches!(result, Err(WatchError::Disabled)),
            "hot_reload = false must refuse the watcher by name, got: {result:?}"
        );
    }

    #[test]
    fn karen_keeps_the_last_good_law_over_a_broken_save() {
        // A gate file saved mid-edit (invalid JSON) must not un-make the
        // working law: the reload fails, the old policies stay in force
        // — and, crucially, she refuses to enforce a PARTIAL law while
        // the disk is lying. Reloads resume the moment the broken file
        // is fixed or removed. Proven through dispatch alone: broken
        // save → standing law still denies → the broken file is fixed
        // and a good gate lands → both laws rule together, no gaps.
        // The Karen who never un-enforces working policy, and never
        // enforces policy she cannot fully read.
        let dir = tempfile::tempdir().expect("temp root");
        let root = minimal_root(dir.path(), true);
        std::fs::write(
            root.join("gates").join("standing-law.json"),
            deny_gate("standing-law", "forbidden"),
        )
        .expect("standing law");
        let set = crate::policy::load(&root).expect("initial load");
        let engine = Engine::new(set, Box::new(MemoryStore::new()));
        let _guard = engine.watch_policies(&root).expect("watcher starts");

        // Baseline: the standing law rules.
        assert!(
            matches!(
                engine.dispatch(&request("forbidden thing")).action,
                Action::Deny { .. }
            ),
            "standing law must deny before the broken save"
        );

        // The broken save arrives. She declines to swap in a partial law.
        std::fs::write(root.join("gates").join("broken.json"), "{ not json").expect("broken save");
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            matches!(
                engine.dispatch(&request("forbidden thing")).action,
                Action::Deny { .. }
            ),
            "the broken save must not un-make the standing law"
        );

        // The edit is fixed the way real edits get fixed — the broken
        // file goes away — and a good gate lands with it. The watcher
        // must still be alive to notice: reloads resume, and both laws
        // rule together, old and new, no gaps between. (Note the
        // needles: the second law's test string must not contain the
        // first law's needle — she reads the whole line, and the test
        // author learned that the hard way, twice.)
        std::fs::remove_file(root.join("gates").join("broken.json")).expect("remove broken file");
        std::fs::write(
            root.join("gates").join("second-law.json"),
            deny_gate("second-law", "unpardonable"),
        )
        .expect("second law");
        let mut second_denied = false;
        for _ in 0..200 {
            let response = engine.dispatch(&request("commit the unpardonable offense"));
            if matches!(response.action, Action::Deny { .. }) {
                assert!(
                    response.fired_policies.iter().any(|id| id == "second-law"),
                    "expected the second law specifically: {:?}",
                    response.fired_policies
                );
                second_denied = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            second_denied,
            "the watcher died on the broken save — it must survive and resume"
        );
        assert!(
            matches!(
                engine.dispatch(&request("forbidden thing")).action,
                Action::Deny { .. }
            ),
            "the standing law must still rule after everything"
        );
    }
}
