use super::StateStore;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// She keeps a file per session. It remembers strikes so you can't open a new window and pretend. The Karen who remembers.
#[derive(Debug)]
pub struct FileStore {
    dir: PathBuf,
    cache: RwLock<HashMap<String, HashMap<String, Value>>>,
}

impl FileStore {
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        // The ledger needs a drawer. She builds one rather than ask whether she should.
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::error!(path = %dir.display(), error = %e, "failed to create state directory");
        }
        Self {
            dir,
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn path_for(&self, session: &str) -> PathBuf {
        let safe: String = session
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{safe}.json"))
    }

    fn load(&self, session: &str) -> HashMap<String, Value> {
        if let Ok(cache) = self.cache.read() {
            if let Some(entries) = cache.get(session) {
                return entries.clone();
            }
        }

        let entries = std::fs::read_to_string(self.path_for(session))
            .ok()
            .and_then(|text| serde_json::from_str::<HashMap<String, Value>>(&text).ok())
            .unwrap_or_default();

        if let Ok(mut cache) = self.cache.write() {
            cache.insert(session.to_string(), entries.clone());
        }
        entries
    }

    fn persist(&self, session: &str, entries: &HashMap<String, Value>) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(session.to_string(), entries.clone());
        }
        let path = self.path_for(session);
        // Same drawer check before every write — a receipt filed nowhere is just a rumor.
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(entries) {
            if let Err(e) = std::fs::write(&path, text) {
                tracing::warn!(path = %path.display(), error = %e, "failed to persist state");
            }
        }
    }

    /// Remove persisted files for sessions untouched since `cutoff`. The Karen who cleans house: untouched for a day means gone, and no, "I was going to get to it" does not count as touching.
    pub fn prune_older_than(&self, cutoff: std::time::SystemTime) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stale = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .is_ok_and(|modified| modified < cutoff);
            if stale {
                remove_file(&path);
            }
        }
    }
}

fn remove_file(path: &Path) {
    if std::fs::remove_file(path).is_ok() {
        tracing::debug!(path = %path.display(), "pruned stale state");
    }
}

impl StateStore for FileStore {
    fn get(&self, session: &str, key: &str) -> Option<Value> {
        self.load(session).get(key).cloned()
    }

    fn set(&self, session: &str, key: &str, value: Value) {
        let mut entries = self.load(session);
        entries.insert(key.to_string(), value);
        self.persist(session, &entries);
    }

    fn remove(&self, session: &str, key: &str) {
        let mut entries = self.load(session);
        entries.remove(key);
        self.persist(session, &entries);
    }
}
