// tests/e2e/common/runtime.rs
// The Karen who checks whether a runtime is even installed before she blames the binary.

use std::process::{Command, Stdio};

/// True when the runtime CLI exists and answers `--version`.
/// The Karen who verifies the guest list before setting the table —
/// no runtime, no test, no drama. She refuses to file a complaint
/// against someone who wasn't even at the party.
pub fn runtime_available(binary: &str) -> bool {
    Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|out| out.status.success())
}
