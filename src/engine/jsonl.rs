//! The ledgers she keeps under `<Karen home>/logs`: one JSON object per line,
//! appended, and emptied into a single `.1` generation past 10 MiB so a
//! long-running runtime does not grow a log without end. The notify log and
//! the runtime-command log share this one pen. The Karen who files every
//! complaint in the same drawer, and empties it when it sticks.

use serde_json::Value;

/// A log rotates to its `.1` generation once it is this big.
const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// Wall-clock milliseconds since the epoch: the `time` of a log line and the
/// value a latch arms with, so a ledger says when something happened, not
/// just that it did.
#[must_use]
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Append `line` as one line of `<Karen home>/logs/<name>`. Past 10 MiB the
/// log moves to `<name>.1` first, replacing the generation already there. A
/// log she cannot write is a log she goes without: a ledger must never be the
/// reason a tool call fails.
pub fn append(name: &str, line: &Value) {
    use std::io::Write;
    let dir = crate::home::karen_home().join("logs");
    let path = dir.join(name);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() >= MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join(format!("{name}.1")));
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let _ = writeln!(file, "{line}");
}

/// `text` on one line: a log line is one line of the log, and a text with a
/// line break in it would become three.
#[must_use]
pub fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
