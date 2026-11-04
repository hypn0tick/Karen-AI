// tests/e2e/host_lean.rs
// The Karen who admits what this build cannot do, instead of pretending. Compiled only into the
// lean build (`--no-default-features`), where `karen run` has no terminal host behind it.

use crate::e2e::common::karen_binary_path;

#[test]
fn karen_run_in_the_lean_build_says_there_is_no_terminal_host() {
    let output = karen::proc::command(karen_binary_path())
        .args(["run", "anything"])
        .output()
        .expect("karen runs");

    assert_eq!(output.status.code(), Some(70));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no terminal host"), "{stderr}");
}
