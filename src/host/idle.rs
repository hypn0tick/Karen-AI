//! Is the hosted session idle? Only then may a delivered line be typed: text typed into the middle
//! of a turn merges into what the agent is typing or gets submitted as part of a different prompt.
//! The Karen who waits her turn, and has been seen to wait through someone else's.
//!
//! Idle takes three things at once:
//!
//! * **quiet**: the runtime has drawn nothing for the configured time (400 ms by default). A
//!   spinner redraws all the time, so a running turn is never quiet;
//! * **a prompt on screen, and empty**: the composer row is found by shape and has nothing typed
//!   after its marker. A half-typed prompt is busy, because a delivery would be appended to it;
//! * **no turn marker**: the runtime's own "press Esc to interrupt" line is not near the composer.
//!
//! Nothing is guessed in the idle direction. A screen with no recognisable prompt is busy, so the
//! worst a wrong profile does is let a command time out and say so, never type into a turn.
//!
//! The profiles are checked as far as there are recorded screens (`tests/fixtures/screens/`):
//! `OpenCode`'s against four screens recorded from a live one, Claude Code's against one screen at
//! rest, the plain rule against a hosted fake runtime, and Codex's against none, so it follows how
//! Codex describes itself. A recorded Codex screen would replace that guess with a check. The `facts.screen.*` extractors in [`super::screen`] read through [`read`] too, so the
//! two cannot disagree about where the prompt is.

use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;

use super::terminal::ScreenDump;

/// The answer, and for a busy one the reason, which is what a timeout's log line could say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Idle,
    Busy(&'static str),
}

/// A composer row: optional box-drawing border, a prompt marker (`>`, `❯`, `›`, `▌`), then whatever
/// has been typed, then optional border. Capture `typed` is that text.
static COMPOSER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\s│┃║|]*[>❯›▌](?: (?P<typed>.*?))?[\s│┃║|]*$")
        .expect("the composer pattern is valid")
});

/// What differs between runtimes: how many non-blank rows may sit under the composer (rules, a
/// status line), and the phrases that mean a turn is running.
struct Profile {
    below: usize,
    busy: &'static [&'static str],
    /// Whether the composer is a box of heavy-bar rows (`OpenCode`) and not a prompt-marker row.
    boxed: bool,
}

const GENERIC: Profile = Profile {
    below: 0,
    busy: &[],
    boxed: false,
};

fn profile(runtime: &str) -> &'static Profile {
    const CLAUDE: Profile = Profile {
        below: 8,
        busy: &["esc to interrupt"],
        boxed: false,
    };
    const CODEX: Profile = Profile {
        below: 5,
        busy: &["esc to interrupt"],
        boxed: false,
    };
    const OPENCODE: Profile = Profile {
        below: 5,
        busy: &["esc interrupt", "esc to interrupt"],
        boxed: true,
    };
    match runtime {
        "claude" => &CLAUDE,
        "codex" => &CODEX,
        "opencode" => &OPENCODE,
        _ => &GENERIC,
    }
}

/// How many rows above the composer are searched for a turn marker. A marker further up is
/// conversation, not state.
const MARKER_WINDOW: usize = 12;

/// The composer row the screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Composer<'a> {
    /// Its row, counted from the top of the screen text.
    pub at: usize,
    /// What has been typed after the prompt marker, trimmed; empty for an empty prompt.
    pub typed: &'a str,
}

/// What a screen shows of the composer and of the turn, read by `runtime`'s profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading<'a> {
    /// The bottom-most composer-shaped row among the last few non-blank rows, when there is one.
    pub composer: Option<Composer<'a>>,
    /// Whether the runtime's own turn marker is near the bottom of the screen; `None` for a
    /// runtime whose profile knows no marker, because no marker seen then says nothing.
    pub running: Option<bool>,
}

/// The bottom-most composer-shaped row among the last few non-blank rows: the prompt marker row of
/// Claude Code, Codex and a plain program.
fn marker_composer<'a>(rows: &[&'a str], below: usize) -> Option<Composer<'a>> {
    let mut seen_below = 0;
    for (at, row) in rows.iter().enumerate().rev() {
        if row.trim().is_empty() {
            continue;
        }
        if let Some(found) = COMPOSER.captures(row) {
            let typed = found.name("typed").map_or("", |m| m.as_str().trim());
            return Some(Composer { at, typed });
        }
        seen_below += 1;
        if seen_below > below {
            break;
        }
    }
    None
}

/// What `OpenCode` shows in an empty composer, which is a hint and not typed text.
const BOXED_PLACEHOLDER: &str = "Ask anything";

/// `OpenCode`'s composer: a box of rows that start with a heavy bar (`┃`), closed underneath by a
/// rule that starts with `╹`. The last bar row is the footer (agent and model); the rows above it
/// are the input. The empty box carries a placeholder on the first screen of a session, which is
/// not typed text.
fn boxed_composer<'a>(rows: &[&'a str]) -> Option<Composer<'a>> {
    let rule = rows
        .iter()
        .rposition(|row| row.trim_start().starts_with('╹'))?;
    let mut top = rule;
    while top > 0 && rows[top - 1].trim_start().starts_with('┃') {
        top -= 1;
    }
    let (_footer, input) = rows[top..rule].split_last()?;
    let typed = input
        .iter()
        .map(|row| row.trim().trim_matches('┃').trim())
        .find(|text| !text.is_empty() && !text.starts_with(BOXED_PLACEHOLDER))
        .unwrap_or("");
    Some(Composer { at: top, typed })
}

/// Read the composer and the turn marker off `text`, the screen as plain text. This is the one
/// reading both [`judge`] and the `facts.screen.*` extractors answer from.
#[must_use]
pub fn read<'a>(runtime: &str, text: &'a str) -> Reading<'a> {
    let profile = profile(runtime);
    let rows: Vec<&str> = text.lines().collect();
    let used = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |last| last + 1);
    let rows = &rows[..used];

    let composer = if profile.boxed {
        boxed_composer(rows)
    } else {
        marker_composer(rows, profile.below)
    };
    // The marker window hangs from the composer, or from the last row when a spinner has taken
    // the composer's place.
    let anchor = composer.map_or(used.saturating_sub(1), |found| found.at);
    let from = anchor.saturating_sub(MARKER_WINDOW);
    let running = (!profile.busy.is_empty()).then(|| {
        rows[from..].iter().any(|row| {
            let row = row.to_lowercase();
            profile.busy.iter().any(|marker| row.contains(marker))
        })
    });
    Reading { composer, running }
}

/// Whether the session behind `screen` is idle, given how long it has been `quiet` and how long it
/// must be (`needed`).
#[must_use]
pub fn judge(runtime: &str, screen: &ScreenDump, quiet: Duration, needed: Duration) -> Verdict {
    if quiet < needed {
        return Verdict::Busy("the runtime is still drawing");
    }
    let reading = read(runtime, &screen.text);
    let Some(composer) = reading.composer else {
        return Verdict::Busy("no prompt on the screen");
    };
    if !composer.typed.is_empty() {
        return Verdict::Busy("something is typed at the prompt");
    }
    if reading.running == Some(true) {
        return Verdict::Busy("a turn is running");
    }
    Verdict::Idle
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUIET: Duration = Duration::from_millis(800);
    const NEEDED: Duration = Duration::from_millis(400);

    fn screen(text: &str) -> ScreenDump {
        ScreenDump {
            text: text.to_string(),
            rows: 40,
            cols: 100,
            cursor_row: 0,
            cursor_col: 0,
            alternate: false,
        }
    }

    fn verdict(runtime: &str, text: &str) -> Verdict {
        judge(runtime, &screen(text), QUIET, NEEDED)
    }

    #[test]
    fn a_plain_empty_prompt_is_idle() {
        assert_eq!(
            verdict("fake-runtime", "fake-runtime ready\n> "),
            Verdict::Idle
        );
        assert_eq!(verdict("fake-runtime", "you said: hi\n>"), Verdict::Idle);
    }

    #[test]
    fn a_half_typed_prompt_is_busy() {
        assert_eq!(
            verdict("fake-runtime", "ready\n> /comp"),
            Verdict::Busy("something is typed at the prompt")
        );
    }

    #[test]
    fn a_runtime_that_is_working_is_busy_because_the_prompt_is_gone() {
        assert_eq!(
            verdict("fake-runtime", "> hello\nworking..."),
            Verdict::Busy("no prompt on the screen")
        );
    }

    #[test]
    fn a_screen_with_no_prompt_is_busy_not_idle() {
        assert_eq!(
            verdict("claude", ""),
            Verdict::Busy("no prompt on the screen")
        );
        assert_eq!(
            verdict("claude", "some output\nmore output"),
            Verdict::Busy("no prompt on the screen")
        );
    }

    #[test]
    fn a_runtime_still_drawing_is_busy_whatever_the_screen_says() {
        assert_eq!(
            judge(
                "fake-runtime",
                &screen("> "),
                Duration::from_millis(100),
                NEEDED
            ),
            Verdict::Busy("the runtime is still drawing")
        );
    }

    #[test]
    fn an_old_prompt_line_in_the_history_is_not_the_composer() {
        let text = "> an old question\nthe answer is long\nand has several lines\nwhat now?";
        assert_eq!(
            verdict("fake-runtime", text),
            Verdict::Busy("no prompt on the screen")
        );
    }

    /// A Claude Code screen at rest, recorded from a live session read-only and scrubbed.
    const CLAUDE_IDLE: &str = include_str!("../../tests/fixtures/screens/claude-idle.txt");

    #[test]
    fn claude_with_an_empty_composer_under_its_rules_is_idle() {
        assert_eq!(verdict("claude", CLAUDE_IDLE), Verdict::Idle);
    }

    #[test]
    fn claude_with_text_in_the_composer_is_busy() {
        let typed = CLAUDE_IDLE.replace("\n❯\n", "\n❯ fix the lint er\n");
        assert_eq!(
            verdict("claude", &typed),
            Verdict::Busy("something is typed at the prompt")
        );
    }

    #[test]
    fn claude_mid_turn_is_busy_even_with_an_empty_composer() {
        let working = CLAUDE_IDLE.replace(
            "✻ Baked for 57s · done",
            "✻ Thinking… (12s · esc to interrupt)",
        );
        assert_eq!(
            verdict("claude", &working),
            Verdict::Busy("a turn is running")
        );
    }

    #[test]
    fn a_turn_marker_far_up_the_screen_is_conversation_not_state() {
        let mut text = String::from("someone wrote: press esc to interrupt\n");
        text.push_str(&"filler\n".repeat(20));
        text.push_str(CLAUDE_IDLE);
        assert_eq!(verdict("claude", &text), Verdict::Idle);
    }

    // Screens recorded from `opencode --model opencode/big-pickle` hosted by `karen run` in a
    // throwaway home (`examples/record-screens.rs`), scrubbed of the working directory.
    const OPENCODE_IDLE: &str = include_str!("../../tests/fixtures/screens/opencode-idle.txt");
    const OPENCODE_HALF: &str =
        include_str!("../../tests/fixtures/screens/opencode-half-typed.txt");
    const OPENCODE_BUSY: &str = include_str!("../../tests/fixtures/screens/opencode-busy.txt");
    const OPENCODE_AFTER: &str =
        include_str!("../../tests/fixtures/screens/opencode-after-turn.txt");

    #[test]
    fn opencode_at_its_first_screen_is_idle_though_its_composer_shows_a_hint() {
        assert_eq!(verdict("opencode", OPENCODE_IDLE), Verdict::Idle);
    }

    #[test]
    fn opencode_after_a_turn_is_idle_with_an_empty_composer() {
        assert_eq!(verdict("opencode", OPENCODE_AFTER), Verdict::Idle);
    }

    #[test]
    fn opencode_with_two_characters_typed_is_busy() {
        assert_eq!(
            verdict("opencode", OPENCODE_HALF),
            Verdict::Busy("something is typed at the prompt")
        );
    }

    #[test]
    fn opencode_mid_turn_is_busy_with_an_empty_composer() {
        assert_eq!(
            verdict("opencode", OPENCODE_BUSY),
            Verdict::Busy("a turn is running")
        );
    }

    #[test]
    fn opencode_without_its_composer_box_is_busy_not_idle() {
        assert_eq!(
            verdict("opencode", "history\n┃  > looks like a prompt\nplain"),
            Verdict::Busy("no prompt on the screen")
        );
    }

    #[test]
    fn a_user_message_in_opencodes_history_is_not_its_composer() {
        let text = format!("┃  an earlier message\n\n{OPENCODE_BUSY}");
        assert_eq!(
            verdict("opencode", &text),
            Verdict::Busy("a turn is running")
        );
    }

    #[test]
    fn the_generic_profile_wants_the_prompt_on_the_last_row() {
        assert_eq!(
            verdict("fake-runtime", "> \nstatus line under it"),
            Verdict::Busy("no prompt on the screen"),
            "an unknown runtime's status rows are not allowed under its prompt"
        );
    }
}
