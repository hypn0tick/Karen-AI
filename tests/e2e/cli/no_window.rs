// tests/e2e/cli/no_window.rs
// The Karen who closes the door quietly: a child she starts opens no window on the owner's desktop.
// On Windows a console program started without a console of its own gets one, and a hook that calls
// a program on every tool call would flash a window on every tool call.

use crate::e2e::common::built_example;

/// The relay is started *detached*: no console at all, like the worker a hook leaves behind. A child
/// of such a process that is started without `CREATE_NO_WINDOW` gets a console window of its own,
/// which is the flash this pins; with the flag it has none.
fn relay(mode: &str) -> String {
    let probe = built_example("console-probe");
    let mut command = std::process::Command::new(&probe);
    command.arg(mode);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS: the relay itself has no console to hand on.
        command.creation_flags(0x0000_0008);
    }
    let output = command.output().expect("the probe runs");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn karen_starts_her_children_without_a_console_window() {
    assert_eq!(
        relay("relay"),
        "window=none",
        "a child started through karen::proc::command has no window, even under a parent with no console"
    );
}

/// `proc::flags` adds creation flags without losing the one that hides the window.
#[test]
fn karen_keeps_the_no_window_flag_when_she_adds_others() {
    assert_eq!(relay("relay-flags"), "window=none");
}

/// Every child she starts goes through `karen::proc`. A new `Command::new` in the library is a
/// window waiting to flash, so the library may not have one.
#[test]
fn karen_has_no_command_new_outside_the_one_that_hides_the_window() {
    let mut offenders = Vec::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![root];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("src is readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && path.file_name().is_some_and(|name| name != "proc.rs")
            {
                let text = std::fs::read_to_string(&path).expect("source is readable");
                // `OPENCODE_PLUGIN_JS` is JavaScript; the bridge hides its own window.
                if path.file_name().is_some_and(|name| name != "main.rs")
                    && text.contains("Command::new(")
                {
                    offenders.push(path.display().to_string());
                }
            }
        }
    }
    assert_eq!(offenders, Vec::<String>::new());
}

/// The terminal layer decides idle from the screen it already holds, in its own process, on a timer
/// that costs nothing. A process per tick would cost a process, a console host and a virus scan
/// each time, so the modules that watch the session may not start one.
#[test]
fn karen_decides_idle_without_starting_a_process() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/host");
    for module in [
        "delivery.rs",
        "idle.rs",
        "terminal.rs",
        "endpoint.rs",
        "screen.rs",
    ] {
        let text = std::fs::read_to_string(root.join(module)).expect("source is readable");
        for forbidden in ["proc::", "std::process", "Command"] {
            assert!(
                !text.contains(forbidden),
                "src/host/{module} watches the session and must not start a process: found `{forbidden}`"
            );
        }
    }
}
