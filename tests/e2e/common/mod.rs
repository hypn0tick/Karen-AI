// tests/e2e/common/mod.rs
// The Karen who builds test infrastructure because she's tired of copy-pasting temp dir setup in every test file.

pub mod assertions;
pub mod fixtures;
pub mod runtime;

// Re-export commonly used types
pub use assertions::*;
pub use fixtures::DEFAULT_CONFIG;
pub use fixtures::DEFAULT_GATES;
pub use fixtures::DEFAULT_RULES;
pub use fixtures::GateFixture;
pub use fixtures::TestEnv;
pub use runtime::runtime_available;

/// The `karen` binary Cargo built for this test run, from the same sources and
/// profile as the tests. The Karen who only signs off on the copy she watched
/// being made: one left in `target/release` by an earlier build may be stale
/// or missing.
pub fn karen_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_karen"))
}

/// Run a command and capture stdout, stderr, and exit code
pub fn run_command(mut cmd: std::process::Command) -> (i32, String, String) {
    let output = cmd.output().expect("failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.code().unwrap_or(-1), stdout, stderr)
}
