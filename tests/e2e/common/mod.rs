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

/// Helper to get the path to the karen binary (release build)
pub fn karen_binary_path() -> std::path::PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let binary_name = if cfg!(windows) { "karen.exe" } else { "karen" };
    std::path::Path::new(&manifest_dir)
        .join("target")
        .join("release")
        .join(binary_name)
}

/// Run a command and capture stdout, stderr, and exit code
pub fn run_command(mut cmd: std::process::Command) -> (i32, String, String) {
    let output = cmd.output().expect("failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.code().unwrap_or(-1), stdout, stderr)
}
