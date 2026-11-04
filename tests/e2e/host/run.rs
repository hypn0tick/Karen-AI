// tests/e2e/host/run.rs
// The Karen who hosts a runtime and is held to account for the terminal she borrowed.

use std::time::Duration;

use karen::host::endpoint::{self, ENV_PID, ENV_RUNTIME};
use karen::host::{Request, Response};

use super::{Hosted, eventually};
use crate::e2e::common::karen_binary_path;

#[test]
fn karen_run_starts_the_runtime_under_conpty_and_draws_its_banner() {
    // The regression for the ConPTY trap: the child does not start until the terminal has answered
    // its cursor-position request. If Karen did not answer at once, `started` would never appear.
    let host = Hosted::start(&[], 50);
    host.await_log("started");

    let screen = host.screen_until("the banner and the prompt", |text| {
        text.contains("fake-runtime ready") && text.trim_end().ends_with('>')
    });
    assert!(
        !screen.contains('\u{1b}'),
        "the screen is plain text, the colours stay on the terminal: {screen:?}"
    );
}

#[test]
fn karen_run_forwards_the_output_as_the_runtime_produced_it() {
    let host = Hosted::start(&[], 50);
    host.await_log("started");
    // Byte-exact output is not promised (ConPTY repaints it), but the runtime's words must arrive.
    eventually("the banner words in the forwarded output", || {
        String::from_utf8_lossy(&host.output())
            .contains("ready")
            .then_some(())
    });
}

#[test]
fn karen_run_forwards_typed_input_byte_for_byte() {
    let mut host = Hosted::start(&[], 50);
    host.await_log("started");
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));

    host.type_into_stdin("héllo /compact \u{4e16}\r".as_bytes());

    host.await_log("line \"héllo /compact \u{4e16}\"");
}

#[test]
fn karen_run_types_a_line_for_a_client_with_the_token() {
    let host = Hosted::start(&[], 50);
    host.await_log("started");
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));

    let answer = host.call(&Request::Type {
        text: "/compact now".into(),
    });

    assert!(answer.ok, "{answer:?}");
    assert_eq!(answer.typed, Some(12));
    host.await_log("line \"/compact now\"");
    host.screen_until("the runtime's reply", |text| {
        text.contains("you said: /compact now")
    });
}

#[test]
fn karen_run_refuses_to_type_anything_but_one_printable_line() {
    let host = Hosted::start(&[], 50);
    host.await_log("started");
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));

    for text in [
        "a\u{1b}[Ab",
        "two\nlines",
        "tab\there",
        "carriage\rreturn",
        "bell\u{7}",
        "",
        "   ",
    ] {
        let answer = host.call(&Request::Type { text: text.into() });
        assert!(!answer.ok, "{text:?} must be refused, got {answer:?}");
        assert!(answer.typed.is_none());
    }

    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !host.log().contains("line "),
        "a refused line reaches nobody: {}",
        host.log()
    );
}

#[test]
fn karen_run_refuses_a_wrong_token() {
    let host = Hosted::start(&[], 50);
    let name = host.env("KAREN_HOST_ENDPOINT");

    let answer = endpoint::call(
        &name,
        "not-the-token",
        &Request::Type {
            text: "/exit".into(),
        },
    )
    .expect("the endpoint answers even a stranger");

    assert_eq!(answer, Response::refused("wrong token"));
    std::thread::sleep(Duration::from_millis(500));
    assert!(!host.log().contains("line "), "the stranger typed nothing");
}

#[test]
fn karen_run_says_who_it_hosts() {
    let mut host = Hosted::start(&[], 50);
    host.await_log("started");
    let status = host.status();

    assert_eq!(status.runtime, "fake-runtime");
    assert_eq!(status.host_pid, host.child.id());
    assert!(status.child_pid.is_some());
    assert_eq!(
        (status.rows, status.cols),
        (24, 80),
        "no terminal: the default size"
    );
    assert_eq!(host.env(ENV_RUNTIME), "fake-runtime");
    assert_eq!(host.env(ENV_PID), host.child.id().to_string());

    std::thread::sleep(Duration::from_millis(600));
    assert!(host.status().quiet_ms >= 400, "an idle runtime grows quiet");
    host.type_into_stdin(b"/exit\r");
    host.exit_code();
}

#[test]
fn karen_run_passes_its_arguments_to_the_runtime_untouched() {
    let mut host = Hosted::start(&["--resume", "-p", "two words"], 50);
    let log = host.await_log("arg \"two words\"");

    assert!(log.lines().any(|l| l == "arg \"--resume\""), "{log}");
    assert!(log.lines().any(|l| l == "arg \"-p\""), "{log}");
    assert!(log.lines().any(|l| l == "arg \"two words\""), "{log}");
    host.type_into_stdin(b"/exit\r");
    host.exit_code();
}

#[test]
fn karen_run_exits_with_the_runtimes_own_exit_code() {
    let mut host = Hosted::start(&[], 50);
    host.await_log("started");
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));

    host.type_into_stdin(b"/crash\r");

    assert_eq!(host.exit_code(), 3);
}

#[test]
fn karen_run_exits_zero_when_the_runtime_does() {
    let mut host = Hosted::start(&[], 50);
    host.await_log("started");
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));

    host.type_into_stdin(b"/exit\r");

    assert_eq!(host.exit_code(), 0);
}

#[test]
fn karen_run_stops_answering_once_the_runtime_is_gone() {
    let mut host = Hosted::start(&[], 50);
    let (name, token) = (
        host.env("KAREN_HOST_ENDPOINT"),
        host.env("KAREN_HOST_TOKEN"),
    );
    host.screen_until("the prompt", |text| text.trim_end().ends_with('>'));
    host.type_into_stdin(b"/exit\r");
    host.exit_code();

    assert!(
        endpoint::call(&name, &token, &Request::Status).is_err(),
        "the endpoint dies with the host"
    );
}

#[test]
fn karen_run_reports_a_runtime_that_is_not_on_path() {
    let output = karen::proc::command(karen_binary_path())
        .args(["run", "no-such-runtime-anywhere"])
        .output()
        .expect("karen runs");

    assert_eq!(output.status.code(), Some(127));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`no-such-runtime-anywhere` is not on PATH"),
        "{stderr}"
    );
    assert!(output.stdout.is_empty(), "stdout belongs to the runtime");
}
