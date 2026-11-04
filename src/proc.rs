//! Starting child processes without putting a window on the owner's desktop. On Windows a console
//! program started by a process that has no console (a detached worker, a service, a hook run by a
//! GUI) gets a console window of its own, and a hook that calls a program on every tool call would
//! flash one on every tool call. `CREATE_NO_WINDOW` stops it. Every child Karen starts, other than
//! the pseudo-console child `karen run` hosts, goes through here. The Karen who closes the door
//! quietly.

use std::ffi::OsStr;
use std::process::Command;

/// The Windows process creation flag that gives a console child no window.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The Windows flag that puts a child in its own process group, so Ctrl-C aimed at the parent does
/// not reach it.
pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// A [`Command`] that opens no window. Off Windows it is a plain `Command`.
#[must_use]
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    hide(&mut command);
    command
}

/// Make `command` open no window. Replaces any creation flags set before it, so set it first and
/// add others with [`flags`].
pub fn hide(command: &mut Command) -> &mut Command {
    flags(command, 0)
}

/// Make `command` open no window, and add the Windows creation flags `extra`.
pub fn flags(command: &mut Command, extra: u32) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW | extra);
    }
    #[cfg(not(windows))]
    let _ = extra;
    command
}
