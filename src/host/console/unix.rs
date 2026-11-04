//! Raw mode on a Unix terminal, in safe Rust.

use rustix::stdio::{stdin, stdout};
use rustix::termios::{OptionalActions, Termios, isatty, tcgetattr, tcgetwinsize, tcsetattr};

/// The attributes the terminal had before, restored on drop.
pub struct Held {
    original: Termios,
}

pub fn enter() -> Option<Held> {
    let input = stdin();
    if !isatty(input) {
        return None;
    }
    let original = tcgetattr(input).ok()?;
    let mut raw = original.clone();
    raw.make_raw();
    tcsetattr(input, OptionalActions::Flush, &raw).ok()?;
    Some(Held { original })
}

impl Drop for Held {
    fn drop(&mut self) {
        // Nothing sensible to do if the terminal has gone away.
        let _ = tcsetattr(stdin(), OptionalActions::Flush, &self.original);
    }
}

pub fn size() -> Option<(u16, u16)> {
    let out = stdout();
    if !isatty(out) {
        return None;
    }
    let size = tcgetwinsize(out).ok()?;
    (size.ws_row > 0 && size.ws_col > 0).then_some((size.ws_row, size.ws_col))
}
