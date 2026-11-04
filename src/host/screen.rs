//! What a hosted runtime's screen says, as facts a gate can match: `facts.screen.*`.
//! The Karen who reads the sign on the door instead of asking the staff what it says.
//!
//! The structured sources come first (the Claude transcript, the Codex rollout, the `OpenCode` SDK);
//! the screen fills what never reaches them: a banner drawn only on screen, a status line's gauge,
//! whether the composer is empty. Each field is an extractor with a name, and a field the screen
//! does not show is **absent**, never a default, so a gate matching one cannot fire on a guess.
//!
//! | field | type | read from |
//! |-------|------|-----------|
//! | `text` | string | the whole visible screen; for `field_regex` only |
//! | `prompt_empty` | bool | the composer row, found as the idle rules find it |
//! | `turn_running` | bool | the runtime's own "esc to interrupt" line; absent for a runtime with no known marker |
//! | `context_used_percent` | number | a bar-style gauge (`Context ███████░░░ 67%`); for `OpenCode` its status row (`8.2K (4%)  ctrl+p commands`) |
//! | `usage_limit` | bool (`true` only) | a usage or rate limit banner near the bottom |
//!
//! Screen text is the runtime's conversation, which can hold anything, so it is never logged, never
//! written to state, and never rendered into a reason or an argument (see `docs/session-facts.md`).
//!
//! The gauge pattern is verified against a recorded Claude Code status line, and the composer and
//! turn rules against screens recorded from a hosted session (`tests/fixtures/screens/`). The
//! banner pattern is provisional: it follows how the runtimes word their limit notices and is not
//! yet checked against a recorded one.

use std::collections::BTreeSet;
use std::sync::LazyLock;
use std::sync::mpsc;
use std::time::Duration;

use regex::Regex;
use serde_json::{Map, Value};

use super::endpoint::{Request, call};
use super::idle;
use super::terminal::ScreenDump;

/// The field names after `facts.screen.`: the list the docs and `karen validate` answer to.
pub const FIELDS: &[&str] = &[
    "text",
    "prompt_empty",
    "turn_running",
    "context_used_percent",
    "usage_limit",
];

/// How many non-blank rows from the bottom a banner is looked for in. A notice further up is
/// conversation scrolled past, not state.
const BANNER_ROWS: usize = 15;

/// A gauge: the word "context", a bar drawn in block or shade characters, then a percentage.
/// The bar is required, so "Context left until auto-compact: 7%" (a different number, the amount
/// left) never reads as the amount used.
static GAUGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bcontext\b\s*[█▓▒░▏▎▍▌▋▊▉■□▪▫]+\s*(\d{1,3})\s*%")
        .expect("the gauge pattern is valid")
});

/// A limit notice: a kind of limit and the word for having reached it, or the limit and when it
/// resets.
static LIMIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:usage|rate|session|weekly|daily|monthly|\d+-hour) limit\b(?:[^\n]{0,40}\b(?:reached|hit|exceeded)\b|[^\n]{0,60}\bresets?\b)|\b(?:reached|hit|exceeded) (?:your|the) [a-z0-9 -]{0,24}\blimit\b",
    )
    .expect("the limit pattern is valid")
});

/// The longest the hook waits for the host to hand over its screen.
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);

/// Ask the host at `target` (`(endpoint, token)`) for its screen. A host that does not answer in
/// time, refuses, or answers with no screen yields nothing: the fact stays absent. The wait runs on
/// a thread of this process, never a child process.
#[must_use]
pub fn fetch(target: &(String, String)) -> Option<ScreenDump> {
    let (name, token) = target.clone();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(call(&name, &token, &Request::Screen));
    });
    match receiver.recv_timeout(FETCH_TIMEOUT) {
        Ok(Ok(answer)) if answer.ok => answer.screen,
        _ => None,
    }
}

/// The fields of `wanted` (names after `facts.screen.`) that `screen` shows, read by `runtime`'s
/// profile. A name that is not a field, or a field the screen does not show, is left out.
#[must_use]
pub fn extract(runtime: &str, screen: &ScreenDump, wanted: &BTreeSet<&str>) -> Map<String, Value> {
    let mut facts = Map::new();
    let mut put = |name: &str, value: Option<Value>| {
        if let (true, Some(value)) = (wanted.contains(name), value) {
            facts.insert(name.to_string(), value);
        }
    };
    put("text", Some(Value::from(screen.text.clone())));
    if wanted.contains("prompt_empty") || wanted.contains("turn_running") {
        let reading = idle::read(runtime, &screen.text);
        put(
            "prompt_empty",
            reading
                .composer
                .map(|composer| Value::from(composer.typed.is_empty())),
        );
        put("turn_running", reading.running.map(Value::from));
    }
    put(
        "context_used_percent",
        context_used_percent(runtime, &screen.text),
    );
    put(
        "usage_limit",
        usage_limit(&screen.text).then(|| Value::from(true)),
    );
    facts
}

/// `OpenCode`'s status row: the tokens in the context and, in brackets, the share of the window
/// they fill, then its key hints: `8.2K (4%)  ctrl+p commands`.
static OPENCODE_GAUGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d+(?:\.\d+)?[KM]?) \((\d{1,3})%\)\s+ctrl\+p commands")
        .expect("the opencode gauge pattern is valid")
});

/// The bottom-most gauge on the screen, as a percentage between 0 and 100: a bar-style gauge for any
/// runtime, and `OpenCode`'s own status row for `OpenCode`.
fn context_used_percent(runtime: &str, text: &str) -> Option<Value> {
    let read = |row: &str| -> Option<u8> {
        if runtime == "opencode" {
            return OPENCODE_GAUGE
                .captures(row)
                .and_then(|found| found[2].parse::<u8>().ok());
        }
        GAUGE
            .captures(row)
            .and_then(|found| found[1].parse::<u8>().ok())
    };
    text.lines()
        .rev()
        .find_map(read)
        .filter(|percent| *percent <= 100)
        .map(Value::from)
}

/// Whether a limit notice sits among the last rows of the screen.
fn usage_limit(text: &str) -> bool {
    text.lines()
        .rev()
        .filter(|row| !row.trim().is_empty())
        .take(BANNER_ROWS)
        .any(|row| LIMIT.is_match(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(text: &str) -> ScreenDump {
        ScreenDump {
            text: text.to_string(),
            rows: 40,
            cols: 100,
            cursor_row: 0,
            cursor_col: 0,
            alternate: false,
        }
    }

    fn all() -> BTreeSet<&'static str> {
        FIELDS.iter().copied().collect()
    }

    fn only(name: &'static str) -> BTreeSet<&'static str> {
        BTreeSet::from([name])
    }

    #[test]
    fn every_field_has_an_extractor_and_only_those_asked_for_are_read() {
        let screen = dump("fake-runtime ready\n> ");
        let got = extract("fake-runtime", &screen, &only("prompt_empty"));
        assert_eq!(got.len(), 1);
        assert_eq!(got["prompt_empty"], Value::from(true));
        assert!(extract("fake-runtime", &screen, &BTreeSet::new()).is_empty());
        let unknown = BTreeSet::from(["nonsense"]);
        assert!(extract("fake-runtime", &screen, &unknown).is_empty());
    }

    #[test]
    fn text_is_the_screen_as_the_host_holds_it() {
        let screen = dump("line one\nline two");
        assert_eq!(
            extract("claude", &screen, &only("text"))["text"],
            Value::from("line one\nline two")
        );
    }

    #[test]
    fn prompt_empty_is_a_bool_when_the_prompt_is_there_and_absent_when_it_is_not() {
        let empty = dump("ready\n> ");
        let typed = dump("ready\n> half a th");
        let working = dump("working...");
        let want = only("prompt_empty");
        assert_eq!(
            extract("fake-runtime", &empty, &want)["prompt_empty"],
            Value::from(true)
        );
        assert_eq!(
            extract("fake-runtime", &typed, &want)["prompt_empty"],
            Value::from(false)
        );
        assert!(extract("fake-runtime", &working, &want).is_empty());
    }

    #[test]
    fn turn_running_follows_the_runtimes_marker_and_is_absent_when_it_has_none() {
        let working = dump("✻ Thinking… (12s · esc to interrupt)\n\n❯\n");
        let idle = dump("✻ Baked for 57s · done\n\n❯\n");
        let want = only("turn_running");
        assert_eq!(
            extract("claude", &working, &want)["turn_running"],
            Value::from(true)
        );
        assert_eq!(
            extract("claude", &idle, &want)["turn_running"],
            Value::from(false)
        );
        assert!(
            extract("fake-runtime", &idle, &want).is_empty(),
            "a runtime with no known marker says nothing about a turn"
        );
    }

    #[test]
    fn the_gauge_reads_a_bar_with_a_percentage_and_nothing_else() {
        let want = only("context_used_percent");
        let gauge = dump("  Context ███████░░░ 67% │ Usage Weekly ████████░░ 82%");
        assert_eq!(
            extract("claude", &gauge, &want)["context_used_percent"],
            Value::from(67)
        );
        let left = dump("Context left until auto-compact: 7%");
        assert!(
            extract("claude", &left, &want).is_empty(),
            "an amount left is not an amount used"
        );
        let silly = dump("Context ███ 140%");
        assert!(extract("claude", &silly, &want).is_empty());
    }

    #[test]
    fn the_bottom_most_gauge_wins() {
        let two = dump("Context ██░░░░░░░░ 20%\n...\nContext ███████░░░ 70%");
        assert_eq!(
            extract("claude", &two, &only("context_used_percent"))["context_used_percent"],
            Value::from(70)
        );
    }

    #[test]
    fn a_limit_banner_near_the_bottom_is_seen_and_one_scrolled_past_is_not() {
        let want = only("usage_limit");
        for banner in [
            "Claude usage limit reached. Your limit will reset at 3pm.",
            "5-hour limit reached · resets 3pm",
            "You've hit your usage limit. Try again later.",
            "Weekly limit reached",
            "Session limit resets in 2h",
        ] {
            let screen = dump(&format!("history\n{banner}\n\n> "));
            assert_eq!(
                extract("claude", &screen, &want)["usage_limit"],
                Value::from(true),
                "{banner}"
            );
        }
        let mut old = String::from("a note: the usage limit reached message\n");
        old.push_str(&"filler\n".repeat(30));
        old.push_str("> ");
        assert!(extract("claude", &dump(&old), &want).is_empty());
        assert!(extract("claude", &dump("all fine\n> "), &want).is_empty());
    }

    const CLAUDE_IDLE: &str = include_str!("../../tests/fixtures/screens/claude-idle.txt");
    const FAKE_READY: &str = include_str!("../../tests/fixtures/screens/fake-runtime-ready.txt");
    const FAKE_GAUGE: &str = include_str!("../../tests/fixtures/screens/fake-runtime-gauge.txt");
    const FAKE_LIMIT: &str = include_str!("../../tests/fixtures/screens/fake-runtime-limit.txt");

    #[test]
    fn a_recorded_claude_screen_at_rest_reads_as_it_looks() {
        let got = extract("claude", &dump(CLAUDE_IDLE), &all());
        assert_eq!(got["prompt_empty"], Value::from(true));
        assert_eq!(got["turn_running"], Value::from(false));
        assert_eq!(got["context_used_percent"], Value::from(67));
        assert!(!got.contains_key("usage_limit"));
    }

    const OPENCODE_IDLE: &str = include_str!("../../tests/fixtures/screens/opencode-idle.txt");
    const OPENCODE_HALF: &str =
        include_str!("../../tests/fixtures/screens/opencode-half-typed.txt");
    const OPENCODE_BUSY: &str = include_str!("../../tests/fixtures/screens/opencode-busy.txt");
    const OPENCODE_AFTER: &str =
        include_str!("../../tests/fixtures/screens/opencode-after-turn.txt");

    const CLAUDE_BUSY: &str = include_str!("../../tests/fixtures/screens/claude-busy.txt");

    #[test]
    fn a_recorded_claude_screen_mid_turn_reads_as_a_turn_in_flight() {
        let got = extract("claude", &dump(CLAUDE_BUSY), &all());
        assert_eq!(got["turn_running"], Value::from(true));
        assert_eq!(got["prompt_empty"], Value::from(true));
        assert_eq!(got["context_used_percent"], Value::from(86));
    }

    #[test]
    fn recorded_opencode_screens_read_as_they_look() {
        let idle = extract("opencode", &dump(OPENCODE_IDLE), &all());
        assert_eq!(
            idle["prompt_empty"],
            Value::from(true),
            "the hint is no text"
        );
        assert_eq!(idle["turn_running"], Value::from(false));
        assert!(!idle.contains_key("context_used_percent"), "no gauge yet");

        let half = extract("opencode", &dump(OPENCODE_HALF), &all());
        assert_eq!(half["prompt_empty"], Value::from(false));

        let busy = extract("opencode", &dump(OPENCODE_BUSY), &all());
        assert_eq!(busy["turn_running"], Value::from(true));
        assert_eq!(busy["prompt_empty"], Value::from(true));

        let after = extract("opencode", &dump(OPENCODE_AFTER), &all());
        assert_eq!(after["prompt_empty"], Value::from(true));
        assert_eq!(after["turn_running"], Value::from(false));
        assert_eq!(after["context_used_percent"], Value::from(4));
        assert!(!after.contains_key("usage_limit"));
    }

    #[test]
    fn opencodes_gauge_is_read_for_opencode_and_not_for_other_runtimes() {
        let want = only("context_used_percent");
        assert!(extract("claude", &dump(OPENCODE_AFTER), &want).is_empty());
        assert!(extract("opencode", &dump(CLAUDE_IDLE), &want).is_empty());
    }

    #[test]
    fn recorded_screens_of_a_hosted_session_read_as_they_look() {
        let ready = extract("fake-runtime", &dump(FAKE_READY), &all());
        assert_eq!(ready["prompt_empty"], Value::from(true));
        assert!(!ready.contains_key("context_used_percent"));
        assert!(!ready.contains_key("usage_limit"));
        assert!(!ready.contains_key("turn_running"), "no known marker");

        let gauge = extract("fake-runtime", &dump(FAKE_GAUGE), &all());
        assert_eq!(gauge["context_used_percent"], Value::from(67));
        assert!(!gauge.contains_key("usage_limit"));

        let limit = extract("fake-runtime", &dump(FAKE_LIMIT), &all());
        assert_eq!(limit["usage_limit"], Value::from(true));
        assert_eq!(limit["prompt_empty"], Value::from(true));
    }

    #[test]
    fn a_full_extraction_reads_every_field_the_screen_shows() {
        let screen = dump("Context ███░░░░░░░ 30%\nready\n> ");
        let got = extract("fake-runtime", &screen, &all());
        let names: Vec<&str> = got.keys().map(String::as_str).collect();
        assert_eq!(names, ["context_used_percent", "prompt_empty", "text"]);
    }
}
