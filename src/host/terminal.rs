//! What the hosted runtime has drawn: a `vt100` screen fed from the pseudo-terminal's output, plus
//! the one question `ConPTY` asks before it will start the child. The Karen who reads the room.

use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};

/// A cursor-position request, `ESC [ 6 n`, recognised byte by byte so it is found even when a read
/// splits it in two. `ConPTY` opens every session with one and holds the child back until it is
/// answered.
#[derive(Debug, Default)]
struct CursorQuery {
    seen: u8,
}

impl CursorQuery {
    const SEQUENCE: [u8; 4] = [0x1b, b'[', b'6', b'n'];

    /// Feed one byte; `true` when it completed a request.
    fn step(&mut self, byte: u8) -> bool {
        let next = usize::from(self.seen);
        if byte == Self::SEQUENCE[next] {
            self.seen += 1;
            if usize::from(self.seen) == Self::SEQUENCE.len() {
                self.seen = 0;
                return true;
            }
        } else {
            self.seen = u8::from(byte == Self::SEQUENCE[0]);
        }
        false
    }
}

/// The visible screen, as a client of the control endpoint sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenDump {
    /// The visible rows as plain text, one per line, with the empty cells at the end of each row
    /// dropped (a space the program wrote on purpose, like the one after a prompt, stays). Never
    /// logged by Karen.
    pub text: String,
    pub rows: u16,
    pub cols: u16,
    /// Zero-based cursor row and column.
    pub cursor_row: u16,
    pub cursor_col: u16,
    /// Whether the runtime is on the alternate screen (a full-screen UI) rather than the main one.
    pub alternate: bool,
}

/// The hosted runtime's screen.
pub struct Terminal {
    parser: vt100::Parser,
    query: CursorQuery,
}

fn nonzero(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN)
}

/// The reply a terminal gives to `ESC [ 6 n`: its cursor, one-based.
fn cursor_report(row: u16, col: u16) -> Vec<u8> {
    format!("\x1b[{};{}R", row.saturating_add(1), col.saturating_add(1)).into_bytes()
}

impl Terminal {
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: vt100::Parser::new(nonzero(rows), nonzero(cols), 0),
            query: CursorQuery::default(),
        }
    }

    /// Draw `bytes` and return what the terminal has to say back to the program that wrote them:
    /// one cursor report per `ESC [ 6 n`, taken at the point the request appeared in the stream.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut replies = Vec::new();
        let mut start = 0;
        for (at, byte) in bytes.iter().enumerate() {
            if self.query.step(*byte) {
                self.parser.process(&bytes[start..=at]);
                start = at + 1;
                let (row, col) = self.parser.screen().cursor_position();
                replies.extend(cursor_report(row, col));
            }
        }
        self.parser.process(&bytes[start..]);
        replies
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.set_size(nonzero(rows), nonzero(cols));
    }

    #[must_use]
    pub fn dump(&self) -> ScreenDump {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let (cursor_row, cursor_col) = screen.cursor_position();
        ScreenDump {
            text: screen.contents(),
            rows: rows.get(),
            cols: cols.get(),
            cursor_row,
            cursor_col,
            alternate: screen.alternate_screen(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_answers_a_cursor_query_from_where_the_cursor_is() {
        let mut term = Terminal::new(24, 80);
        let reply = term.feed(b"hello\r\nab\x1b[6n");
        assert_eq!(reply, b"\x1b[2;3R", "row 2, column 3, one-based");
    }

    #[test]
    fn terminal_answers_a_cursor_query_split_across_reads() {
        let mut term = Terminal::new(24, 80);
        assert_eq!(term.feed(b"x\x1b["), Vec::<u8>::new());
        assert_eq!(term.feed(b"6"), Vec::<u8>::new());
        assert_eq!(term.feed(b"n"), b"\x1b[1;2R");
    }

    #[test]
    fn terminal_answers_every_query_in_one_read() {
        let mut term = Terminal::new(24, 80);
        let reply = term.feed(b"\x1b[6nab\x1b[6n");
        assert_eq!(
            reply, b"\x1b[1;1R\x1b[1;3R",
            "each answered where it was asked"
        );
    }

    #[test]
    fn terminal_ignores_other_escape_sequences() {
        let mut term = Terminal::new(24, 80);
        assert_eq!(
            term.feed(b"\x1b[5n\x1b[6m\x1b[?9001h\x1b[6"),
            Vec::<u8>::new()
        );
        assert_eq!(term.feed(b"x"), Vec::<u8>::new());
    }

    #[test]
    fn terminal_dump_is_plain_text_without_the_colours() {
        let mut term = Terminal::new(5, 40);
        term.feed(b"\x1b[1;32mfake-runtime\x1b[0m ready\r\n> ");
        let dump = term.dump();
        assert_eq!(dump.text, "fake-runtime ready\n> ");
        assert_eq!((dump.rows, dump.cols), (5, 40));
        assert_eq!((dump.cursor_row, dump.cursor_col), (1, 2));
        assert!(!dump.alternate);
    }

    #[test]
    fn terminal_follows_a_resize() {
        let mut term = Terminal::new(24, 80);
        term.resize(10, 30);
        assert_eq!((term.dump().rows, term.dump().cols), (10, 30));
    }
}
