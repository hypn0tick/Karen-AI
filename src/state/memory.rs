use super::StateStore;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-lifetime store. Fast, but she doesn't persist. The Karen who, for once, forgets — and wants it in writing that it was on purpose.
#[derive(Debug, Default)]
pub struct MemoryStore {
    entries: Mutex<HashMap<String, Value>>,
}

impl MemoryStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn key(session: &str, key: &str) -> String {
        format!("{session}\u{1f}{key}")
    }
}

impl StateStore for MemoryStore {
    fn get(&self, session: &str, key: &str) -> Option<Value> {
        self.entries
            .lock()
            .ok()?
            .get(&Self::key(session, key))
            .cloned()
    }

    fn set(&self, session: &str, key: &str, value: Value) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(Self::key(session, key), value);
        }
    }

    fn remove(&self, session: &str, key: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(&Self::key(session, key));
        }
    }
}
