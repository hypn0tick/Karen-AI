// tests/e2e/host/attach.rs
// The Karen who attaches to a console she did not open: `examples/attach-probe.rs` against the
// fake runtime hosted by `karen run`. The host is only the thing that makes a pseudo-console to
// attach to; the probe never speaks to its endpoint. Windows only: console attach is a Windows API.
//
// The probe is its own process because a process has one console, and attaching takes it from the
// one that was there. Nothing types into an agent.

use serial_test::serial;

use super::{Hosted, eventually};
use crate::e2e::common::built_example;

/// The id the fake runtime logged for itself: the process whose console the probe attaches to.
fn runtime_pid(host: &Hosted) -> u32 {
    eventually("the runtime to log its process id", || {
        host.log()
            .lines()
            .find_map(|line| line.strip_prefix("pid ")?.parse().ok())
    })
}

/// Run the probe against `pid` with `args`, as the code it exited with and what it printed.
fn probe(pid: u32, args: &[&str]) -> (Option<i32>, String, String) {
    let output = karen::proc::command(built_example("attach-probe"))
        .arg(pid.to_string())
        .args(args)
        .output()
        .expect("the probe runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
#[serial(host_delivery)]
fn a_process_outside_the_session_reads_its_screen_buffer() {
    let host = Hosted::start(&[], 50);
    host.await_log("started");
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));

    let (code, stdout, stderr) = probe(runtime_pid(&host), &["--read"]);

    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("fake-runtime ready"), "{stdout}");
    assert!(
        stdout.trim_end().ends_with('>'),
        "the prompt is there: {stdout}"
    );
}

#[test]
#[serial(host_delivery)]
fn a_process_outside_the_session_types_a_line_and_submits_it() {
    let host = Hosted::start(&[], 50);
    host.await_log("started");
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));

    let (code, stdout, stderr) = probe(
        runtime_pid(&host),
        &["--type", "from outside", "--enter", "--read"],
    );

    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("typed 12 characters"), "{stdout}");
    host.await_log("line \"from outside\"");
    host.screen_until("the runtime's answer", |text| {
        text.contains("you said: from outside")
    });
}

#[test]
#[serial(host_delivery)]
fn attaching_to_a_process_that_is_gone_fails_and_says_so() {
    let (code, _stdout, stderr) = probe(u32::MAX - 1, &["--read"]);

    assert_eq!(code, Some(2));
    assert!(stderr.contains("AttachConsole"), "{stderr}");
}
