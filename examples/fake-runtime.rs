//! A stand-in for an agent CLI, for testing `karen run` without putting a real agent in harm's way.
//! It paints a banner, shows a `> ` prompt, takes a line, "works" on it for a moment (no prompt
//! while it does), answers, and shows the prompt again. The Karen who rehearses on a dummy first.
//!
//! * `FAKE_RUNTIME_LOG`: a file. Every line the runtime receives is appended to it exactly as it
//!   arrived (`line "…"`), and so is each argument it was started with (`arg "…"`) and each
//!   `KAREN_HOST_*` variable it inherited (`env NAME=value`), which is how a test learns where the
//!   control endpoint is.
//! * `FAKE_RUNTIME_WORK_MS`: how long the runtime is busy with a line (default 300).
//! * `/exit` ends it with code 0, `/crash` with code 3.
//!
//! Build and run through `cargo run --example fake-runtime`; `karen run` hosts it like any other.

use std::io::{BufRead, Write};
use std::time::Duration;

fn note(line: &str) {
    let Some(path) = std::env::var_os("FAKE_RUNTIME_LOG") else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

fn say(text: &str) {
    let mut out = std::io::stdout();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn main() {
    let work = std::env::var("FAKE_RUNTIME_WORK_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .unwrap_or(300);

    note("started");
    for argument in std::env::args().skip(1) {
        note(&format!("arg {argument:?}"));
    }
    for (name, value) in std::env::vars().filter(|(name, _)| name.starts_with("KAREN_HOST_")) {
        note(&format!("env {name}={value}"));
    }

    say("\x1b[2J\x1b[H\x1b[1;32mfake-runtime\x1b[0m ready\r\n> ");
    let mut line = String::new();
    while std::io::stdin().lock().read_line(&mut line).unwrap_or(0) > 0 {
        let heard = line.trim_end_matches(['\r', '\n']).to_string();
        line.clear();
        note(&format!("line {heard:?}"));
        match heard.as_str() {
            "/exit" => return,
            "/crash" => std::process::exit(3),
            _ => {}
        }
        say("\r\nworking...");
        std::thread::sleep(Duration::from_millis(work));
        say(&format!("\r\nyou said: {heard}\r\n> "));
    }
}
