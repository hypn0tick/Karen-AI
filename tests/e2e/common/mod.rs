// tests/e2e/common/mod.rs
// The Karen who builds test infrastructure because she's tired of copy-pasting temp dir setup in every test file.

pub mod assertions;
pub mod commands;
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

/// An example from this package, built fresh beside the `karen` binary. `cargo test --test test_e2e`
/// builds the binary but not the examples, and a stale example would make a test lie, so each is
/// built (a no-op when current) the first time it is asked for.
pub fn built_example(name: &str) -> std::path::PathBuf {
    let mut build = karen::proc::command(env!("CARGO"));
    build
        .args(["build", "--quiet", "--example", name])
        .arg("--manifest-path")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
    if !cfg!(debug_assertions) {
        build.arg("--release");
    }
    assert!(
        build.status().expect("cargo runs").success(),
        "{name} must build"
    );
    let path = karen_binary_path()
        .parent()
        .expect("the binary has a directory")
        .join("examples")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(path.exists(), "{} is missing", path.display());
    path
}

/// Run a command and capture stdout, stderr, and exit code
pub fn run_command(mut cmd: std::process::Command) -> (i32, String, String) {
    let output = cmd.output().expect("failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.code().unwrap_or(-1), stdout, stderr)
}
