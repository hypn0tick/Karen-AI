//! Attach to the console of a process Karen did not start, read its screen buffer, and type into
//! its input buffer. A probe, not a feature: it answers whether a session that was started any
//! other way (a plain terminal, Windows Terminal, a multiplexer) could be reached the way
//! `karen run` reaches one it owns. The findings are in `docs/console-attach.md`.
//!
//! ```text
//! attach-probe PID [--read] [--type TEXT] [--enter] [--enter-delay-ms 150]
//! ```
//!
//! * `--read` prints the visible rows of the console `PID` is attached to, as the console's own
//!   screen buffer holds them (no escape sequences: the console has already interpreted them);
//! * `--type TEXT` writes TEXT into the console's input buffer as key events, one down and one up
//!   per character;
//! * `--enter` then presses Enter, after the pause, because a terminal UI that reads a burst of
//!   input as a paste would take the Enter for part of it.
//!
//! What it does to the process that runs it: a process has one console, so it lets go of its own
//! (`FreeConsole`) and takes `PID`'s (`AttachConsole`). Run it as its own process, never from
//! inside a session you care about. It only works on Windows; elsewhere it says so.
//!
//! It types into whatever `PID`'s console is, with the same rights the owner has over it, which is
//! why Karen does not do this by itself. Point it at a throwaway session.

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::fmt::Write as _;
    use std::fs::OpenOptions;
    use std::io::Write as _;
    use std::os::windows::io::AsRawHandle;
    use std::process::ExitCode;
    use std::time::Duration;

    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, CONSOLE_SCREEN_BUFFER_INFO, COORD, FreeConsole,
        GetConsoleMode, GetConsoleScreenBufferInfo, INPUT_RECORD, KEY_EVENT,
        ReadConsoleOutputCharacterW, WriteConsoleInputW,
    };

    const VK_RETURN: u16 = 0x0D;

    struct Options {
        pid: u32,
        read: bool,
        text: Option<String>,
        enter: bool,
        delay: Duration,
    }

    fn options(args: &[String]) -> Result<Options, String> {
        let pid = args
            .first()
            .and_then(|pid| pid.parse().ok())
            .ok_or("the first argument is the process id")?;
        let mut options = Options {
            pid,
            read: false,
            text: None,
            enter: false,
            delay: Duration::from_millis(150),
        };
        let mut at = 1;
        while at < args.len() {
            match args[at].as_str() {
                "--read" => options.read = true,
                "--enter" => options.enter = true,
                "--type" => {
                    at += 1;
                    options.text = Some(args.get(at).ok_or("--type wants text")?.clone());
                }
                "--enter-delay-ms" => {
                    at += 1;
                    let ms = args.get(at).and_then(|ms| ms.parse().ok());
                    options.delay = Duration::from_millis(ms.ok_or("--enter-delay-ms wants ms")?);
                }
                other => return Err(format!("unknown argument {other}")),
            }
            at += 1;
        }
        Ok(options)
    }

    /// The visible rows of the screen buffer behind `handle`, trailing blanks dropped.
    fn read_screen(handle: *mut std::ffi::c_void) -> Result<Vec<String>, String> {
        // SAFETY: CONSOLE_SCREEN_BUFFER_INFO is plain data that the call fills in.
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
        // SAFETY: `handle` is a live console output handle and `info` a valid out pointer.
        if unsafe { GetConsoleScreenBufferInfo(handle, &raw mut info) } == 0 {
            return Err(format!(
                "GetConsoleScreenBufferInfo: {}",
                std::io::Error::last_os_error()
            ));
        }
        let width = usize::try_from(info.dwSize.X).map_err(|why| why.to_string())?;
        let mut rows = Vec::new();
        for y in info.srWindow.Top..=info.srWindow.Bottom {
            let mut cells = vec![0_u16; width];
            let mut read = 0_u32;
            // SAFETY: `cells` holds `width` elements, the length passed; `read` is a valid out
            // pointer; `handle` is a live console output handle.
            let ok = unsafe {
                ReadConsoleOutputCharacterW(
                    handle,
                    cells.as_mut_ptr(),
                    u32::try_from(width).map_err(|why| why.to_string())?,
                    COORD { X: 0, Y: y },
                    &raw mut read,
                )
            };
            if ok == 0 {
                return Err(format!(
                    "ReadConsoleOutputCharacterW: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let row = String::from_utf16_lossy(&cells[..read as usize]);
            rows.push(row.trim_end().to_string());
        }
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        Ok(rows)
    }

    /// One key press and release for `unit`, with `virtual_key` when it is not a character.
    fn key(unit: u16, virtual_key: u16, down: bool) -> INPUT_RECORD {
        // SAFETY: INPUT_RECORD is plain data; zero is a valid starting value for every field.
        let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
        record.EventType = u16::try_from(KEY_EVENT).expect("KEY_EVENT fits in u16");
        // EventType is KEY_EVENT, so the KeyEvent arm of the union is the one in use. Writing a
        // union field is safe; only reading one is not.
        record.Event.KeyEvent.bKeyDown = i32::from(down);
        record.Event.KeyEvent.wRepeatCount = 1;
        record.Event.KeyEvent.wVirtualKeyCode = virtual_key;
        record.Event.KeyEvent.uChar.UnicodeChar = unit;
        record
    }

    fn write_input(handle: *mut std::ffi::c_void, records: &[INPUT_RECORD]) -> Result<(), String> {
        let mut written = 0_u32;
        // SAFETY: `records` holds the length passed; `written` is a valid out pointer; `handle` is
        // a live console input handle.
        let ok = unsafe {
            WriteConsoleInputW(
                handle,
                records.as_ptr(),
                u32::try_from(records.len()).map_err(|why| why.to_string())?,
                &raw mut written,
            )
        };
        if ok == 0 {
            return Err(format!(
                "WriteConsoleInputW: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    /// What the probe has to say, held back until it has left the target's console: while attached
    /// it has no console of its own, and a standard handle that was the old console is closed.
    #[derive(Default)]
    struct Report {
        out: String,
        err: String,
    }

    fn attempt(options: &Options, report: &mut Report) -> Result<(), String> {
        // SAFETY: both calls take no pointers. A process has one console, so this one lets go of
        // its own before it takes the target's.
        let attached = unsafe {
            FreeConsole();
            AttachConsole(options.pid)
        };
        if attached == 0 {
            return Err(format!(
                "AttachConsole({}): {}",
                options.pid,
                std::io::Error::last_os_error()
            ));
        }
        if options.read {
            let screen = OpenOptions::new()
                .read(true)
                .write(true)
                .open("CONOUT$")
                .map_err(|why| format!("CONOUT$: {why}"))?;
            for row in read_screen(screen.as_raw_handle())? {
                let _ = writeln!(report.out, "{row}");
            }
        }
        if options.text.is_some() || options.enter {
            let input = OpenOptions::new()
                .read(true)
                .write(true)
                .open("CONIN$")
                .map_err(|why| format!("CONIN$: {why}"))?;
            if let Some(text) = &options.text {
                let mut records = Vec::new();
                for unit in text.encode_utf16() {
                    records.push(key(unit, 0, true));
                    records.push(key(unit, 0, false));
                }
                write_input(input.as_raw_handle(), &records)?;
                let _ = writeln!(report.out, "typed {} characters", text.chars().count());
            }
            if options.enter {
                std::thread::sleep(options.delay);
                let enter = [key(0x0D, VK_RETURN, true), key(0x0D, VK_RETURN, false)];
                write_input(input.as_raw_handle(), &enter)?;
                let _ = writeln!(report.out, "pressed Enter");
            }
        }
        Ok(())
    }

    fn is_console(handle: *mut std::ffi::c_void) -> bool {
        let mut mode = 0_u32;
        // SAFETY: `mode` is a valid out pointer; a handle that is not a console just fails.
        unsafe { GetConsoleMode(handle, &raw mut mode) != 0 }
    }

    /// Say it. A standard stream that was a console is written through the parent's console,
    /// re-attached; one that was a pipe or a file is written as it is, because letting go of a
    /// console does not touch those.
    fn emit(report: &Report, stdout_console: bool, stderr_console: bool) {
        if stdout_console || stderr_console {
            // SAFETY: neither call takes a pointer. This process has already finished with the
            // target's console.
            unsafe {
                FreeConsole();
                AttachConsole(ATTACH_PARENT_PROCESS);
            }
        }
        let through_console = |text: &str| {
            if let Ok(mut console) = OpenOptions::new().write(true).open("CONOUT$") {
                let _ = console.write_all(text.as_bytes());
            }
        };
        if stdout_console {
            through_console(&report.out);
        } else {
            let _ = std::io::stdout().write_all(report.out.as_bytes());
        }
        if stderr_console {
            through_console(&report.err);
        } else {
            let _ = std::io::stderr().write_all(report.err.as_bytes());
        }
    }

    pub fn run() -> ExitCode {
        let stdout_console = is_console(std::io::stdout().as_raw_handle());
        let stderr_console = is_console(std::io::stderr().as_raw_handle());
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut report = Report::default();
        let result = options(&args).and_then(|options| attempt(&options, &mut report));
        if let Err(why) = &result {
            let _ = writeln!(report.err, "attach-probe: {why}");
        }
        emit(&report, stdout_console, stderr_console);
        if result.is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        }
    }
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    imp::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("attach-probe: console attach is a Windows API; there is nothing to probe here");
}
