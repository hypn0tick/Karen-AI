//! Raw mode on a Windows console. This is the one module in Karen that allows `unsafe`: the console
//! API is `SetConsoleMode` and friends, and there is no safe wrapper in `std`.
//!
//! What is set: the input handle gets `ENABLE_VIRTUAL_TERMINAL_INPUT` and nothing else (no line
//! editing, no echo, no Ctrl-C processing, so every keystroke arrives as the bytes a terminal would
//! send), and the output handle gets virtual terminal processing so the hosted runtime's escape
//! sequences draw instead of printing. Both modes are captured first and put back by `Drop`, and by
//! a console control handler when the console is closed or Ctrl-Break arrives.

#![allow(unsafe_code)]

use std::os::windows::io::AsRawHandle;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{FALSE, TRUE};
use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_INPUT,
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetConsoleScreenBufferInfo,
    SetConsoleCtrlHandler, SetConsoleMode,
};

/// Modes to put back, as `(handle, original mode)`. The control handler runs on a thread of
/// Windows' choosing, so it reads this rather than a guard.
static ARMED: Mutex<Vec<(isize, u32)>> = Mutex::new(Vec::new());

/// The modes captured on entry, restored on drop.
pub struct Held {
    handles: Vec<(isize, u32)>,
}

fn mode_of(handle: isize) -> Option<u32> {
    let mut mode = 0_u32;
    // SAFETY: `mode` is a valid out pointer for the whole call, and an invalid handle makes the
    // call fail (returning 0) rather than misbehave.
    let ok = unsafe { GetConsoleMode(handle as _, &raw mut mode) };
    (ok != 0).then_some(mode)
}

fn set_mode(handle: isize, mode: u32) -> bool {
    // SAFETY: plain value arguments; a handle that is not a console makes the call fail.
    unsafe { SetConsoleMode(handle as _, mode) != 0 }
}

/// Restore every armed mode. `try_lock`, because this also runs on the control handler's thread
/// and must never wait for the thread it interrupted.
fn restore_armed() {
    if let Ok(mut armed) = ARMED.try_lock() {
        for (handle, original) in armed.drain(..) {
            set_mode(handle, original);
        }
    }
}

/// Called by Windows on Ctrl-C, Ctrl-Break and console close. Puts the modes back and says it did
/// not handle the event, so the default action (ending the process) still happens.
unsafe extern "system" fn on_control(_kind: u32) -> i32 {
    restore_armed();
    FALSE
}

/// Put `input` and (when it is a console) `output` into the hosting modes.
pub fn enter_on(input: isize, output: Option<isize>) -> Option<Held> {
    let input_original = mode_of(input)?;
    if !set_mode(input, ENABLE_VIRTUAL_TERMINAL_INPUT) {
        return None;
    }
    let mut handles = vec![(input, input_original)];
    if let Some(output) = output
        && let Some(original) = mode_of(output)
    {
        let wanted = original | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        if set_mode(output, wanted) {
            handles.push((output, original));
        }
    }
    if let Ok(mut armed) = ARMED.lock() {
        armed.clone_from(&handles);
    }
    // SAFETY: `on_control` is an `extern "system"` function with the signature Windows expects and
    // lives for the whole process.
    unsafe { SetConsoleCtrlHandler(Some(on_control), TRUE) };
    Some(Held { handles })
}

pub fn enter() -> Option<Held> {
    let input = std::io::stdin().as_raw_handle() as isize;
    let output = std::io::stdout().as_raw_handle() as isize;
    enter_on(input, Some(output))
}

impl Drop for Held {
    fn drop(&mut self) {
        // SAFETY: removes the handler added in `enter_on`; the same function pointer identifies it.
        unsafe { SetConsoleCtrlHandler(Some(on_control), FALSE) };
        if let Ok(mut armed) = ARMED.lock() {
            armed.clear();
        }
        for (handle, original) in &self.handles {
            set_mode(*handle, *original);
        }
    }
}

pub fn size() -> Option<(u16, u16)> {
    let output = std::io::stdout().as_raw_handle() as isize;
    // SAFETY: an all-zero `CONSOLE_SCREEN_BUFFER_INFO` is a valid value of this plain-data struct.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a valid out pointer for the whole call; a handle that is not a console
    // makes the call fail.
    let ok = unsafe { GetConsoleScreenBufferInfo(output as _, &raw mut info) };
    if ok == 0 {
        return None;
    }
    let rows = i32::from(info.srWindow.Bottom) - i32::from(info.srWindow.Top) + 1;
    let cols = i32::from(info.srWindow.Right) - i32::from(info.srWindow.Left) + 1;
    Some((u16::try_from(rows).ok()?, u16::try_from(cols).ok()?)).filter(|(r, c)| *r > 0 && *c > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs::OpenOptions;

    /// `CONIN$` and `CONOUT$`, or `None` (with the reason printed) when this process has no console:
    /// a CI runner, a service, or a harness that redirected everything.
    fn console() -> Option<(std::fs::File, std::fs::File)> {
        let open = |name: &str| OpenOptions::new().read(true).write(true).open(name);
        match (open("CONIN$"), open("CONOUT$")) {
            (Ok(input), Ok(output)) => Some((input, output)),
            (Err(why), _) | (_, Err(why)) => {
                eprintln!("skipped: no console is attached to this process ({why})");
                None
            }
        }
    }

    // Serial: both tests change the one console this process has, and the control handler's table.
    #[test]
    #[serial]
    fn raw_mode_is_restored_after_a_panic() {
        let Some((input, output)) = console() else {
            return;
        };
        let (i, o) = (
            input.as_raw_handle() as isize,
            output.as_raw_handle() as isize,
        );
        let (before_in, before_out) = (mode_of(i), mode_of(o));

        let outcome = std::panic::catch_unwind(|| {
            let _held = enter_on(i, Some(o)).expect("a console accepts raw mode");
            assert_eq!(mode_of(i), Some(ENABLE_VIRTUAL_TERMINAL_INPUT));
            panic!("a hosted runtime has a bad day");
        });

        assert!(outcome.is_err(), "the panic was meant to unwind");
        assert_eq!(mode_of(i), before_in, "input mode comes back after a panic");
        assert_eq!(
            mode_of(o),
            before_out,
            "output mode comes back after a panic"
        );
    }

    #[test]
    #[serial]
    fn raw_mode_is_restored_by_the_control_handler() {
        let Some((input, output)) = console() else {
            return;
        };
        let (i, o) = (
            input.as_raw_handle() as isize,
            output.as_raw_handle() as isize,
        );
        let before = mode_of(i);
        let held = enter_on(i, Some(o)).expect("a console accepts raw mode");

        // What Windows calls on Ctrl-Break: the guard is still alive and the process is about to die.
        assert_eq!(
            unsafe { on_control(1) },
            FALSE,
            "the default action must still run"
        );
        assert_eq!(mode_of(i), before, "the handler restored the input mode");
        drop(held);
    }
}
