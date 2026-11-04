//! `karen run`: hosting a runtime in a pseudo-terminal Karen owns. The Karen who holds the terminal,
//! and also the door.
//!
//! Five threads share one [`Host`]:
//!
//! * the **reader** copies the runtime's output to the real terminal and draws it on the `vt100`
//!   screen (answering `ConPTY`'s cursor query as it goes);
//! * the **keyboard** copies the real terminal's input to the runtime, byte for byte;
//! * the **endpoint** answers `karen hook` processes;
//! * the **resizer** follows the real terminal's size;
//! * the main thread waits for the runtime to exit and returns its exit code.
//!
//! Lock order: no thread ever holds two of the host's locks at once, so there is no order to get
//! wrong. The writer lock is held for a whole line, which is what keeps an injected line from being
//! sliced into the middle of someone's typing.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use super::console;
use super::delivery::{self, Queue, Tuning};
use super::endpoint::{
    self, ENV_ENDPOINT, ENV_PID, ENV_RUNTIME, ENV_TOKEN, EndpointError, Handler, HostStatus,
    Request, Response,
};
use super::program;
use super::terminal::{ScreenDump, Terminal};
use crate::engine::runtime_command::{is_typeable, status};
use crate::policy::{Roots, loader::HostConfig};

/// How often the delivery worker looks at the screen.
const DELIVERY_POLL: Duration = Duration::from_millis(100);
/// How long output may stay silent before the runtime's last words are considered delivered.
const DRAIN_QUIET: Duration = Duration::from_millis(150);
/// The most she waits for the last words.
const DRAIN_MAX: Duration = Duration::from_millis(1500);
/// How often she looks at the real terminal's size.
const RESIZE_POLL: Duration = Duration::from_millis(250);

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("`{0}` is not on PATH, and she looked")]
    NotFound(String),
    #[error("cannot open a pseudo-terminal or start the runtime in it: {0}")]
    Pty(String),
    #[error(transparent)]
    Endpoint(#[from] EndpointError),
    #[error("cannot read the terminal: {0}")]
    Io(#[from] std::io::Error),
}

/// What to host.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// The program to start: `claude`, `codex`, `opencode`, or any other command line tool.
    pub program: OsString,
    pub args: Vec<OsString>,
    /// The runtime to record the program as (`KAREN_HOST_RUNTIME`) when its file name is not the
    /// runtime's name: a wrapper script, a renamed binary. Unset, the program's file stem.
    pub name: Option<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Everything the threads share.
struct Host {
    runtime: String,
    child_pid: Option<u32>,
    terminal: Mutex<Terminal>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    last_output: Mutex<Instant>,
    queue: Arc<Queue>,
    enter_delay: Duration,
}

impl Host {
    fn quiet(&self) -> Duration {
        lock(&self.last_output).elapsed()
    }

    fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut writer = lock(&self.writer);
        writer.write_all(bytes)?;
        writer.flush()
    }

    /// Why `text` is not a line she will type, or `None` when it is: one printable line, not blank.
    /// A control character, or a newline, would be a keystroke she was not asked to make.
    fn refusal(text: &str) -> Option<Response> {
        if text.trim().is_empty() {
            return Some(Response::refused("nothing to type"));
        }
        if !is_typeable(text) {
            return Some(Response::refused(
                "one printable line only: no control characters, no newlines, at most 1000 characters",
            ));
        }
        None
    }

    /// Type `text`, wait out the Enter delay, press Enter. The writer stays locked throughout, so
    /// nobody's typing lands in the middle of the line.
    fn press_line(&self, text: &str) -> std::io::Result<()> {
        let mut writer = lock(&self.writer);
        writer.write_all(text.as_bytes())?;
        writer.flush()?;
        std::thread::sleep(self.enter_delay);
        writer.write_all(b"\r")?;
        writer.flush()
    }

    /// Type `text` and press Enter, now.
    fn type_line(&self, text: &str) -> Response {
        if let Some(refusal) = Self::refusal(text) {
            return refusal;
        }
        match self.press_line(text) {
            Ok(()) => Response {
                typed: Some(text.chars().count()),
                ..Response::ok()
            },
            Err(why) => Response::refused(format!("the terminal would not take it: {why}")),
        }
    }

    /// Join the queue; the worker types it when the session is idle.
    fn deliver(&self, text: &str, meta: &endpoint::DeliveryMeta) -> Response {
        if let Some(refusal) = Self::refusal(text) {
            return refusal;
        }
        if endpoint::arrived_late(meta.sent_at_ms, endpoint::now_ms()) {
            return Response::refused("arrived after the hook stopped waiting; not typed");
        }
        match self.queue.push(text.to_string(), meta.clone()) {
            Some(place) => Response {
                queued: Some(place),
                ..Response::ok()
            },
            None => Response::refused("the queue is full"),
        }
    }
}

impl delivery::Session for Host {
    fn runtime(&self) -> &str {
        &self.runtime
    }

    fn screen(&self) -> ScreenDump {
        lock(&self.terminal).dump()
    }

    fn quiet(&self) -> Duration {
        Host::quiet(self)
    }

    fn press(&self, text: &str) -> Result<(), String> {
        self.press_line(text).map_err(|why| why.to_string())
    }
}

impl Handler for Host {
    fn handle(&self, request: &Request) -> Response {
        match request {
            Request::Status => {
                let dump = lock(&self.terminal).dump();
                Response {
                    status: Some(HostStatus {
                        runtime: self.runtime.clone(),
                        host_pid: std::process::id(),
                        child_pid: self.child_pid,
                        rows: dump.rows,
                        cols: dump.cols,
                        quiet_ms: u64::try_from(self.quiet().as_millis()).unwrap_or(u64::MAX),
                        queued: self.queue.len(),
                    }),
                    ..Response::ok()
                }
            }
            Request::Screen => Response {
                screen: Some(lock(&self.terminal).dump()),
                ..Response::ok()
            },
            Request::Type { text } => self.type_line(text),
            Request::Deliver { text, meta } => self.deliver(text, meta),
        }
    }
}

fn pty_error(why: impl std::fmt::Display) -> RunError {
    RunError::Pty(why.to_string())
}

fn size_of(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Copy the runtime's output to the real terminal and onto the screen.
fn pump_output(host: &Arc<Host>, mut reader: Box<dyn Read + Send>) {
    let mut buffer = [0_u8; 8192];
    while let Ok(read) = reader.read(&mut buffer) {
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        *lock(&host.last_output) = Instant::now();
        let replies = lock(&host.terminal).feed(chunk);
        if !replies.is_empty() {
            let _ = host.write(&replies);
        }
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(chunk).and_then(|()| out.flush());
    }
}

/// Copy the real terminal's input to the runtime, untouched.
fn pump_input(host: &Arc<Host>) {
    let mut buffer = [0_u8; 4096];
    let mut input = std::io::stdin();
    while let Ok(read) = input.read(&mut buffer) {
        if read == 0 || host.write(&buffer[..read]).is_err() {
            break;
        }
    }
}

/// Keep the pseudo-terminal and the screen the size of the real terminal.
fn follow_size(host: &Arc<Host>, mut current: (u16, u16)) {
    loop {
        std::thread::sleep(RESIZE_POLL);
        if let Some(size) = console::size().filter(|size| *size != current) {
            current = size;
            let _ = lock(&host.master).resize(size_of(size.0, size.1));
            lock(&host.terminal).resize(size.0, size.1);
        }
    }
}

/// The owner's `[host]` settings: the user layer's alone, and the defaults when it cannot be read.
/// A policy that fails to load must not stop a runtime from starting.
fn host_config() -> HostConfig {
    let roots = Roots {
        user: Some(crate::home::user_policies()).filter(|path| path.is_dir()),
        project: None,
        explicit: false,
        found_by: None,
    };
    crate::policy::load_roots(&roots)
        .map(|set| set.config.host)
        .unwrap_or_default()
}

/// Host `options.program` until it exits, and return its exit code.
///
/// # Errors
///
/// When the program is not on `PATH`, a pseudo-terminal cannot be opened, or the control endpoint
/// cannot be created. Nothing is started in the first two cases.
pub fn run(options: &RunOptions) -> Result<i32, RunError> {
    let path = std::env::var_os("PATH");
    let launch = program::launch(&options.program, path.as_deref())
        .ok_or_else(|| RunError::NotFound(options.program.to_string_lossy().into_owned()))?;
    let runtime = options
        .name
        .clone()
        .unwrap_or_else(|| program::runtime_name(&options.program));
    let config = host_config();

    let (rows, cols) = console::size().unwrap_or((24, 80));
    let pair = native_pty_system()
        .openpty(size_of(rows, cols))
        .map_err(pty_error)?;

    let (listener, endpoint) = endpoint::bind()?;
    let mut command = CommandBuilder::new(&launch.program);
    command.args(&launch.prefix);
    command.args(&options.args);
    if let Ok(directory) = std::env::current_dir() {
        command.cwd(directory);
    }
    command.env(ENV_ENDPOINT, &endpoint.name);
    command.env(ENV_TOKEN, &endpoint.token);
    command.env(ENV_RUNTIME, &runtime);
    command.env(ENV_PID, std::process::id().to_string());

    let mut child = pair.slave.spawn_command(command).map_err(|why| {
        endpoint.close();
        pty_error(why)
    })?;
    drop(pair.slave);
    let reader = pair.master.try_clone_reader().map_err(pty_error)?;
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut writer = pair.master.take_writer().map_err(pty_error)?;

    // ConPTY opens with a cursor-position request and starts nothing until it is answered. The first
    // answer goes in at once, because an answer that waited for the first read arrived too late in
    // the spike that found this. The reader also answers the request from the screen's real cursor
    // (see `Terminal::feed`), and either answer alone unsticks the child on the machine that ran the
    // tests; with neither the child never starts, which is what the banner test pins down.
    #[cfg(windows)]
    {
        writer.write_all(b"\x1b[1;1R")?;
        writer.flush()?;
    }

    let host = Arc::new(Host {
        runtime,
        child_pid: child.process_id(),
        terminal: Mutex::new(Terminal::new(rows, cols)),
        writer: Mutex::new(writer),
        master: Mutex::new(pair.master),
        last_output: Mutex::new(Instant::now()),
        queue: Arc::new(Queue::default()),
        enter_delay: Duration::from_millis(config.enter_delay_ms),
    });

    endpoint::serve(listener, endpoint.token.clone(), host.clone());
    let stop = Arc::new(AtomicBool::new(false));
    let sink = delivery::log_sink();
    {
        let (session, queue) = (
            Arc::clone(&host) as Arc<dyn delivery::Session>,
            Arc::clone(&host.queue),
        );
        let (sink, stop) = (Arc::clone(&sink), Arc::clone(&stop));
        let tuning = Tuning {
            quiet: Duration::from_millis(config.quiet_ms),
            timeout: Duration::from_secs(config.timeout_secs),
            poll: DELIVERY_POLL,
        };
        std::thread::spawn(move || delivery::run_worker(&session, &queue, tuning, &sink, &stop));
    }
    let raw_mode = console::enter_raw();

    let output = {
        let host = Arc::clone(&host);
        std::thread::spawn(move || pump_output(&host, reader))
    };
    {
        let host = Arc::clone(&host);
        std::thread::spawn(move || pump_input(&host));
    }
    {
        let host = Arc::clone(&host);
        std::thread::spawn(move || follow_size(&host, (rows, cols)));
    }

    let status = child.wait();

    // The runtime's last words are still in the pipe: wait for them to stop arriving.
    let waited = Instant::now();
    while host.quiet() < DRAIN_QUIET && waited.elapsed() < DRAIN_MAX {
        std::thread::sleep(Duration::from_millis(25));
    }
    stop.store(true, Ordering::SeqCst);
    delivery::drain(&host.queue, &sink, status::HOST_EXITED);
    drop(raw_mode);
    endpoint.close();
    // The reader ends by itself where the pty reports the end of output, and not at all under
    // ConPTY until the process exits; neither is waited for.
    drop(output);

    let status = status.map_err(|why| RunError::Pty(why.to_string()))?;
    Ok(i32::try_from(status.exit_code()).unwrap_or(1))
}
