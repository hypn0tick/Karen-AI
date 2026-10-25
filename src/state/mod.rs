//! State backends she trusts, which is a short list. The Karen who stores, files, and never loses anything — including grievances.

mod file;
mod memory;

pub use file::FileStore;
pub use memory::MemoryStore;

use serde_json::Value;

/// Session-scoped key/value state. The Karen who tracks strikes on paper, in triplicate, with a pen that never runs out.
pub trait StateStore: Send + Sync {
    fn get(&self, session: &str, key: &str) -> Option<Value>;
    fn set(&self, session: &str, key: &str, value: Value);
    fn remove(&self, session: &str, key: &str);
    fn increment(&self, session: &str, key: &str) -> u64 {
        let next = self.get(session, key).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
        self.set(session, key, Value::from(next));
        next
    }
}

/// She reads your file but writes nothing, like a guest who rearranges nothing.
/// The Karen who previews a strike without recording it, because a dry run that
/// eats your only warning is not a dry run — it's sabotage with a smile.
pub struct ReadOnlyStore {
    inner: Box<dyn StateStore>,
}

impl ReadOnlyStore {
    #[must_use]
    pub fn new(inner: Box<dyn StateStore>) -> Self {
        Self { inner }
    }
}

impl StateStore for ReadOnlyStore {
    fn get(&self, session: &str, key: &str) -> Option<Value> {
        self.inner.get(session, key)
    }

    fn set(&self, _session: &str, _key: &str, _value: Value) {
        // Writes refused. The Karen who window-shops and touches nothing.
    }

    fn remove(&self, _session: &str, _key: &str) {
        // Removal refused too. She's just looking, and she'll leave the
        // shelf exactly as she found it.
    }

    fn increment(&self, session: &str, key: &str) -> u64 {
        // Preview the strike the NEXT real call would record, without
        // recording it. The Karen who counts your strikes on her fingers —
        // in ink that washes off, and with the real ledger locked in her bag.
        self.inner
            .get(session, key)
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            + 1
    }
}
