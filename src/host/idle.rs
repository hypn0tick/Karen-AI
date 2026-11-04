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
//! The profiles for Claude Code, Codex and `OpenCode` are provisional: Claude Code's from one
//! recorded screen, the other two from how those tools document themselves. They are refined
//! against recorded screens in the same change as the screen facts.

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
}

const GENERIC: Profile = Profile {
    below: 0,
    busy: &[],
};

fn profile(runtime: &str) -> &'static Profile {
    const CLAUDE: Profile = Profile {
        below: 8,
        busy: &["esc to interrupt"],
    };
    const CODEX: Profile = Profile {
        below: 5,
        busy: &["esc to interrupt"],
    };
    const OPENCODE: Profile = Profile {
        below: 5,
        busy: &["esc interrupt", "esc to interrupt"],
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

/// Whether the session behind `screen` is idle, given how long it has been `quiet` and how long it
/// must be (`needed`).
#[must_use]
pub fn judge(runtime: &str, screen: &ScreenDump, quiet: Duration, needed: Duration) -> Verdict {
    if quiet < needed {
        return Verdict::Busy("the runtime is still drawing");
    }
    let profile = profile(runtime);
    let rows: Vec<&str> = screen.text.lines().collect();
    let used = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |last| last + 1);
    let rows = &rows[..used];

    // The composer is the bottom-most composer-shaped row among the last few non-blank rows.
    let mut seen_below = 0;
    let mut composer = None;
    for (at, row) in rows.iter().enumerate().rev() {
        if row.trim().is_empty() {
            continue;
        }
        if let Some(found) = COMPOSER.captures(row) {
            let typed = found.name("typed").map_or("", |m| m.as_str().trim());
            composer = Some((at, typed));
            break;
        }
        seen_below += 1;
        if seen_below > profile.below {
            break;
        }
    }
    let Some((at, typed)) = composer else {
        return Verdict::Busy("no prompt on the screen");
    };
    if !typed.is_empty() {
        return Verdict::Busy("something is typed at the prompt");
    }
    let from = at.saturating_sub(MARKER_WINDOW);
    let running = rows[from..].iter().any(|row| {
        let row = row.to_lowercase();
        profile.busy.iter().any(|marker| row.contains(marker))
    });
    if running {
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

    const CLAUDE_IDLE: &str = "\
● Done with the thing

✻ Baked for 57s · done

──────────────────────────────────────
❯
──────────────────────────────────────
  [Opus 5.5] │ project git:(main*)
  Context ███████░░░ 67% │ Usage Weekly ████████░░ 82%
  ⏵⏵ bypass permissions on";

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

    #[test]
    fn a_boxed_composer_with_a_border_is_found() {
        assert_eq!(
            verdict(
                "opencode",
                "history\n┃  >                    ┃\n  esc to quit"
            ),
            Verdict::Idle
        );
        assert_eq!(
            verdict("opencode", "history\n┃  > half a thought    ┃"),
            Verdict::Busy("something is typed at the prompt")
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
