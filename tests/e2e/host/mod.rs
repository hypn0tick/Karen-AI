// tests/e2e/host/mod.rs
// The Karen who hosts the fake runtime, never a real one: `karen run` against `examples/fake-runtime`.
//
// Every test here starts the real `karen` binary, which opens a real pseudo-terminal (ConPTY on
// Windows) and starts the real fake runtime in it. Nothing types into an agent.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use karen::host::endpoint::{self, HostStatus};
use karen::host::{Request, Response};

use crate::e2e::common::{built_example, karen_binary_path};

#[cfg(test)]
mod deliver;
#[cfg(test)]
mod run;
#[cfg(test)]
mod screens;

/// How long any one wait may take. `ConPTY` is not quick to start on a busy machine.
const PATIENCE: Duration = Duration::from_secs(20);

/// The fake runtime, built fresh beside the `karen` binary the first time it is asked for.
fn fake_runtime() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(|| built_example("fake-runtime")).clone()
}

/// Poll `probe` until it returns `Some`, or fail with `what` after [`PATIENCE`].
fn eventually<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(found) = probe() {
            return found;
        }
        assert!(Instant::now() < deadline, "gave up waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A `karen run` hosting the fake runtime, with its stdin held open and its stdout drained.
struct Hosted {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
    log: tempfile::NamedTempFile,
    output: Arc<Mutex<Vec<u8>>>,
    /// The home `start` made, kept until the host is dropped.
    home: Option<tempfile::TempDir>,
}

impl Hosted {
    /// Host the fake runtime with `args` after it, busy for `work_ms` per line.
    fn start(args: &[&str], work_ms: u64) -> Self {
        let home = tempfile::tempdir().expect("a home");
        let mut host = Self::start_in(home.path(), None, args, work_ms);
        host.home = Some(home);
        host
    }

    /// Host it with `home` as its home (so it reads that user layer and files in that runtime log),
    /// recorded as runtime `name` when given.
    fn start_in(home: &Path, name: Option<&str>, args: &[&str], work_ms: u64) -> Self {
        let log = tempfile::NamedTempFile::new().expect("a log file");
        let mut command = karen::proc::command(karen_binary_path());
        command.arg("run");
        if let Some(name) = name {
            command.args(["--as", name]);
        }
        command
            .arg(fake_runtime())
            .args(args)
            .env("HOME", home)
            .env("USERPROFILE", home)
            .env_remove("KAREN_HOME")
            .env_remove("KAREN_POLICIES")
            .env("FAKE_RUNTIME_LOG", log.path())
            .env("FAKE_RUNTIME_WORK_MS", work_ms.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().expect("karen run starts");
        let stdin = child.stdin.take();
        let output = Arc::new(Mutex::new(Vec::new()));
        let mut stdout = child.stdout.take().expect("piped stdout");
        let sink = Arc::clone(&output);
        std::thread::spawn(move || {
            let mut chunk = [0_u8; 4096];
            while let Ok(read) = std::io::Read::read(&mut stdout, &mut chunk) {
                if read == 0 {
                    break;
                }
                sink.lock().unwrap().extend_from_slice(&chunk[..read]);
            }
        });
        Self {
            child,
            stdin,
            log,
            output,
            home: None,
        }
    }

    /// Everything the fake runtime has written to its log so far.
    fn log(&self) -> String {
        std::fs::read_to_string(self.log.path()).unwrap_or_default()
    }

    /// Wait for the runtime to log `line`, and return the whole log.
    fn await_log(&self, line: &str) -> String {
        eventually(&format!("the runtime to log `{line}`"), || {
            let log = self.log();
            log.lines().any(|l| l == line).then_some(log)
        })
    }

    /// The `KAREN_HOST_*` variable the runtime inherited, as it logged it.
    fn env(&self, name: &str) -> String {
        let prefix = format!("env {name}=");
        eventually(&format!("the runtime to report {name}"), || {
            self.log()
                .lines()
                .find_map(|l| l.strip_prefix(&prefix).map(str::to_string))
        })
    }

    fn call(&self, request: &Request) -> Response {
        let (name, token) = (
            self.env("KAREN_HOST_ENDPOINT"),
            self.env("KAREN_HOST_TOKEN"),
        );
        endpoint::call(&name, &token, request).expect("the endpoint answers")
    }

    fn status(&self) -> HostStatus {
        self.call(&Request::Status)
            .status
            .expect("a status answer carries a status")
    }

    /// The screen text, once `wanted` holds for it.
    fn screen_until(&self, what: &str, wanted: impl Fn(&str) -> bool) -> String {
        eventually(what, || {
            let screen = self.call(&Request::Screen).screen?;
            wanted(&screen.text).then_some(screen.text)
        })
    }

    fn type_into_stdin(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin.write_all(bytes).expect("karen takes input");
        stdin.flush().expect("flush");
    }

    /// What `karen run` wrote to its own stdout (the runtime's output, as produced).
    fn output(&self) -> Vec<u8> {
        self.output.lock().unwrap().clone()
    }

    /// Wait for `karen run` to exit and return its exit code.
    fn exit_code(&mut self) -> i32 {
        eventually("karen run to exit", || {
            self.child
                .try_wait()
                .expect("try_wait")
                .map(|s| s.code().unwrap_or(-1))
        })
    }
}

impl Drop for Hosted {
    fn drop(&mut self) {
        // A test that failed leaves a host behind; take it, and the fake runtime in its terminal,
        // with it.
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
