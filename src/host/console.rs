//! The terminal Karen herself is sitting in: put it in raw mode while she hosts a runtime, and put
//! it back however she leaves. The Karen who borrows your terminal and returns it as found.
//!
//! The Unix arm is safe Rust (`rustix` wraps `termios`). The Windows arm needs `SetConsoleMode`, so
//! it lives in `windows.rs`, the one module in the crate that allows `unsafe`, with a `SAFETY`
//! comment on each call.
//!
//! The mode is restored by `Drop`, which covers a normal exit, an early return and a caught panic.
//! On Windows a console control handler also restores it on Ctrl-Break and a closing console. A
//! hard kill (`taskkill /f`, `kill -9`) runs neither, and leaves the terminal raw: `reset` (Unix)
//! or opening a new console (Windows) puts it right.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

/// Raw mode, held. Dropping it restores the terminal.
#[must_use = "dropping the guard is what restores the terminal"]
pub struct RawMode {
    _held: imp::Held,
}

/// Put the terminal Karen was started in into raw mode, so keystrokes reach the hosted runtime
/// untouched. `None` when stdin is not a terminal (a pipe, a test, a hook): there is nothing to
/// change and bytes pass through as they are.
#[must_use]
pub fn enter_raw() -> Option<RawMode> {
    imp::enter().map(|held| RawMode { _held: held })
}

/// The visible size of the terminal as `(rows, cols)`, or `None` when stdout is not a terminal.
#[must_use]
pub fn size() -> Option<(u16, u16)> {
    imp::size()
}
