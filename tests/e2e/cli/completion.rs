// tests/e2e/cli/completion.rs
// The Karen who completes completions because tab completion shouldn't be a mystery.

use crate::e2e::common::{TestEnv, assert_exit_code};
use serial_test::serial;

#[test]
#[serial]
fn karen_completion_bash_outputs_valid_script() {
    // The Karen who verifies bash completion outputs valid script.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["completion", "bash"]);
    assert_exit_code(code, 0);
    assert!(stdout.contains("karen"));
    assert!(stdout.contains("complete"));
    assert!(stdout.len() > 100); // Should be substantial
}

#[test]
#[serial]
fn karen_completion_zsh_outputs_valid_script() {
    // The Karen who verifies zsh completion outputs valid script.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["completion", "zsh"]);
    assert_exit_code(code, 0);
    assert!(stdout.contains("karen"));
    assert!(stdout.contains("_karen"));
    assert!(stdout.len() > 100);
}

#[test]
#[serial]
fn karen_completion_fish_outputs_valid_script() {
    // The Karen who verifies fish completion outputs valid script.
    let env = TestEnv::new();
    let (code, stdout, _stderr) = env.run_karen(&["completion", "fish"]);
    assert_exit_code(code, 0);
    assert!(stdout.contains("karen"));
    assert!(stdout.contains("complete"));
    assert!(stdout.len() > 100);
}
