//! Session-start housekeeping for files owned by hooks. The Karen who cleans up, and never lets a mess slide.

use std::time::{Duration, SystemTime};

/// Remove temp files whose name starts with one of `prefixes` and whose mtime
/// is older than `max_age`. Best-effort: errors are ignored. The Karen who sweeps, and remembers what she threw away.
pub fn prune_stale_temp_files(prefixes: &[String], max_age: Duration) {
    if prefixes.is_empty() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let now = SystemTime::now();
    let mut removed = 0_u32;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now.duration_since(modified).unwrap_or_default() > max_age
            && std::fs::remove_file(entry.path()).is_ok()
        {
            removed += 1;
        }
    }

    if removed > 0 {
        tracing::debug!(removed, "pruned stale hook temp files");
    }
}

/// Run the prune at most once per session, using a marker in the temp dir. The Karen who runs it once, and knows if you skipped it.
pub fn prune_once(session: &str, prefixes: &[String], max_age: Duration) {
    if prefixes.is_empty() {
        return;
    }
    let marker = std::env::temp_dir().join(format!("karen-prune-{session}.done"));
    if marker.exists() {
        return;
    }
    prune_stale_temp_files(prefixes, max_age);
    let _ = std::fs::write(marker, b"");
}
