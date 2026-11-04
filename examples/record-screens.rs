//! Record the screens of a real agent CLI hosted by `karen run`, in a throwaway home, for the
//! `facts.screen.*` extractors and the idle rules to be checked against. The Karen who photographs
//! the building before she writes the fire code.
//!
//! ```text
//! cargo run --example record-screens -- --runtime opencode --out DIR \
//!     [--prompt "reply with the single word ok"] [--deliver | --attach] [--env KEY=VALUE]... \
//!     [--wait-secs 90] -- opencode --model opencode/big-pickle
//! ```
//!
//! With `--attach` (Windows) the prompt is typed from outside, through the console API
//! (`examples/attach-probe.rs`), and not through the host at all.
//!
//! With `--deliver` the prompt, and a second one, go through the host's queue (`deliver`) and not
//! straight to the keyboard (`type`), so the host's idle rules decide when each is typed, and the
//! host's own account of what it did is printed at the end.
//!
//! It starts `karen run --as <runtime> <this example> --relay …` with a temporary `HOME` (and the
//! XDG directories under it), so the program starts with no configuration of yours; pass `--env` to
//! hand it exactly the variables it needs, such as a free model's key. The relay writes the
//! `KAREN_HOST_*` variables the program inherits to a file and then runs it, which is how this
//! process learns the endpoint and token. It then records:
//!
//! * `<runtime>-idle.txt`: the screen once the program has drawn its prompt and stopped drawing;
//! * `<runtime>-half-typed.txt`: the same with two characters typed and not submitted;
//! * `<runtime>-busy.txt`: the screen about a second into a turn it submits with `--prompt`;
//! * `<runtime>-after-turn.txt`: the screen once the turn is over and the program is quiet again;
//! * `<runtime>-trace/NN.txt`: every distinct screen it saw during the turn, in order.
//!
//! The program types into itself only what this tool types: the two characters, and the prompt.
//! Rows are saved without trailing spaces. The screens are what the program drew in that home:
//! read them before they are kept, because a path, a name or an account can be on them.
//!
//! The relay runs the program with plain `std::process::Command`, not `karen::proc::command`: the
//! program is the pty's own child and must share its console.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitCode, Stdio};
use std::time::{Duration, Instant};

use karen::host::Request;
use karen::host::endpoint::{self, DeliveryMeta};

/// How long the screen must hold still to count as at rest.
const AT_REST: Duration = Duration::from_millis(1500);
const POLL: Duration = Duration::from_millis(200);

struct Options {
    runtime: String,
    out: PathBuf,
    karen: PathBuf,
    prompt: String,
    /// Hand the prompt, and a second one, to the host's idle-gated queue instead of typing it now.
    deliver: bool,
    /// Type the prompt through the Windows console API from outside, not through the host.
    attach: bool,
    env: Vec<(String, String)>,
    wait: Duration,
    program: Vec<String>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--relay") {
        return relay(&args[1..]);
    }
    match options(&args).and_then(|options| record(&options)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("record-screens: {why}");
            ExitCode::from(2)
        }
    }
}

/// Run as the pty's child: say where the endpoint is, then become the program's parent.
fn relay(args: &[String]) -> ExitCode {
    let [file, program, rest @ ..] = args else {
        eprintln!("record-screens --relay FILE PROGRAM [ARGS]...");
        return ExitCode::from(2);
    };
    let mut lines: Vec<String> = std::env::vars()
        .filter(|(name, _)| name.starts_with("KAREN_HOST_"))
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    // The relay shares the program's console, so it is a process whose console can be attached to.
    lines.push(format!("RELAY_PID={}", std::process::id()));
    if std::fs::write(file, lines.join("\n")).is_err() {
        return ExitCode::from(2);
    }
    match std::process::Command::new(program).args(rest).status() {
        Ok(status) => ExitCode::from(u8::try_from(status.code().unwrap_or(1)).unwrap_or(1)),
        Err(why) => {
            eprintln!("cannot run {program}: {why}");
            ExitCode::from(127)
        }
    }
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut runtime = None;
    let mut out = None;
    let mut karen = None;
    let mut prompt = "reply with the single word ok".to_string();
    let mut env = Vec::new();
    let mut deliver = false;
    let mut attach = false;
    let mut wait = Duration::from_secs(90);
    let mut at = 0;
    while at < args.len() {
        let flag = args[at].as_str();
        if flag == "--" {
            at += 1;
            break;
        }
        if flag == "--deliver" || flag == "--attach" {
            if flag == "--deliver" {
                deliver = true;
            } else {
                attach = true;
            }
            at += 1;
            continue;
        }
        let value = args
            .get(at + 1)
            .ok_or_else(|| format!("{flag} wants a value"))?;
        match flag {
            "--runtime" => runtime = Some(value.clone()),
            "--out" => out = Some(PathBuf::from(value)),
            "--karen" => karen = Some(PathBuf::from(value)),
            "--prompt" => prompt.clone_from(value),
            "--wait-secs" => {
                let secs = value.parse().map_err(|_| "--wait-secs wants a number")?;
                wait = Duration::from_secs(secs);
            }
            "--env" => {
                let (key, val) = value.split_once('=').ok_or("--env wants KEY=VALUE")?;
                env.push((key.to_string(), val.to_string()));
            }
            other => return Err(format!("unknown flag {other}")),
        }
        at += 2;
    }
    let program = args[at..].to_vec();
    if program.is_empty() {
        return Err("name the program after `--`".into());
    }
    let karen = karen.map_or_else(sibling_karen, Ok)?;
    Ok(Options {
        runtime: runtime.ok_or("--runtime is required")?,
        out: out.ok_or("--out is required")?,
        karen,
        prompt,
        deliver,
        attach,
        env,
        wait,
        program,
    })
}

/// The `karen` binary beside the `examples` directory this example was built into.
fn sibling_karen() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|why| why.to_string())?;
    let name = if cfg!(windows) { "karen.exe" } else { "karen" };
    let karen = exe
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.join(name))
        .ok_or("cannot find the karen binary; pass --karen")?;
    if karen.exists() {
        Ok(karen)
    } else {
        Err(format!(
            "{} does not exist; build karen or pass --karen",
            karen.display()
        ))
    }
}

/// A `karen run` hosting the program, with its output drained and its stdin held for typing.
struct Session {
    child: Child,
    target: (String, String),
    /// A process on the program's console, for `--attach`.
    relay_pid: u32,
}

impl Session {
    fn screen(&self) -> Option<String> {
        let answer = endpoint::call(&self.target.0, &self.target.1, &Request::Screen).ok()?;
        answer.screen.map(|screen| kept(&screen.text))
    }

    fn type_line(&self, text: &str) -> Result<(), String> {
        let request = Request::Type {
            text: text.to_string(),
        };
        let answer = endpoint::call(&self.target.0, &self.target.1, &request)
            .map_err(|why| why.to_string())?;
        if answer.ok {
            Ok(())
        } else {
            Err(format!("the host refused the line: {:?}", answer.error))
        }
    }

    /// Type `text` and Enter through the Windows console API, from outside the session
    /// (`attach-probe`), and not through the host.
    fn attach_line(&self, text: &str) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|why| why.to_string())?;
        let probe = exe.with_file_name(if cfg!(windows) {
            "attach-probe.exe"
        } else {
            "attach-probe"
        });
        let output = karen::proc::command(&probe)
            .arg(self.relay_pid.to_string())
            .args(["--type", text, "--enter"])
            .output()
            .map_err(|why| format!("{}: {why}", probe.display()))?;
        println!(
            "attach-probe: {} {}",
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        if output.status.success() {
            Ok(())
        } else {
            Err("attach-probe failed".into())
        }
    }

    /// Hand `text` to the host's queue, which types it when the program is idle. Answers with the
    /// place in the queue.
    fn deliver_line(&self, runtime: &str, text: &str) -> Result<usize, String> {
        let request = Request::Deliver {
            text: text.to_string(),
            meta: DeliveryMeta {
                session: "record-screens".into(),
                gate: "record-screens".into(),
                runtime: runtime.into(),
                command: text.into(),
                arguments: None,
            },
        };
        let answer = endpoint::call(&self.target.0, &self.target.1, &request)
            .map_err(|why| why.to_string())?;
        match (answer.ok, answer.queued) {
            (true, Some(place)) => Ok(place),
            _ => Err(format!("the host refused the line: {:?}", answer.error)),
        }
    }

    fn keys(&mut self, bytes: &[u8]) {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let _ = stdin.write_all(bytes);
            let _ = stdin.flush();
        }
    }

    /// The screen once it has held still for [`AT_REST`], or whatever it last was at `deadline`.
    fn at_rest(&self, deadline: Instant) -> Option<String> {
        let mut last: Option<String> = None;
        let mut since = Instant::now();
        while Instant::now() < deadline {
            let now = self.screen();
            let drawn = now.as_deref().is_some_and(|text| !text.trim().is_empty());
            if drawn && now == last {
                if since.elapsed() >= AT_REST {
                    return now;
                }
            } else {
                last = now;
                since = Instant::now();
            }
            std::thread::sleep(POLL);
        }
        last
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A screen as it is kept: no trailing spaces on a row, none after the last row.
fn kept(text: &str) -> String {
    let rows: Vec<&str> = text.lines().map(str::trim_end).collect();
    let used = rows
        .iter()
        .rposition(|row| !row.is_empty())
        .map_or(0, |last| last + 1);
    format!("{}\n", rows[..used].join("\n"))
}

fn start(options: &Options, home: &Path) -> Result<Session, String> {
    let relay_file = home.join("host-env.txt");
    let mut command = karen::proc::command(&options.karen);
    command
        .args(["run", "--as", &options.runtime])
        .arg(std::env::current_exe().map_err(|why| why.to_string())?)
        .arg("--relay")
        .arg(&relay_file)
        .args(&options.program)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env_remove("KAREN_HOME")
        .env_remove("KAREN_POLICIES")
        .envs(options.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|why| format!("karen run: {why}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    // The host draws the program's output here; nobody reads it, but it must not fill up.
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let (target, relay_pid) = loop {
        if let Some(found) = read_target(&relay_file) {
            break found;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            return Err("the program never reported its host endpoint".into());
        }
        std::thread::sleep(POLL);
    };
    Ok(Session {
        child,
        target,
        relay_pid,
    })
}

fn read_target(file: &Path) -> Option<((String, String), u32)> {
    let text = std::fs::read_to_string(file).ok()?;
    let find = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{name}=")))
            .map(str::to_string)
    };
    Some((
        (find("KAREN_HOST_ENDPOINT")?, find("KAREN_HOST_TOKEN")?),
        find("RELAY_PID")?.parse().ok()?,
    ))
}

fn save(out: &Path, name: &str, text: &str) -> Result<(), String> {
    let path = out.join(name);
    std::fs::create_dir_all(path.parent().unwrap_or(out)).map_err(|why| why.to_string())?;
    std::fs::write(&path, text).map_err(|why| format!("{}: {why}", path.display()))?;
    println!("recorded {}", path.display());
    Ok(())
}

fn record(options: &Options) -> Result<(), String> {
    let home = tempfile::tempdir().map_err(|why| why.to_string())?;
    let mut session = start(options, home.path())?;
    let runtime = options.runtime.as_str();
    let out = options.out.as_path();
    let startup = Instant::now() + Duration::from_secs(60);

    let idle = session.at_rest(startup).ok_or("the program drew nothing")?;
    save(out, &format!("{runtime}-idle.txt"), &idle)?;

    session.keys(b"he");
    let half = session
        .at_rest(Instant::now() + Duration::from_secs(15))
        .ok_or("no screen after typing")?;
    save(out, &format!("{runtime}-half-typed.txt"), &half)?;
    // Two backspaces (DEL), so the composer is empty again whatever the program does with Ctrl-U.
    session.keys(&[0x7f, 0x7f]);
    session.at_rest(Instant::now() + Duration::from_secs(15));

    if options.attach {
        session.attach_line(&options.prompt)?;
    } else if options.deliver {
        let first = session.deliver_line(runtime, &options.prompt)?;
        let second = session.deliver_line(runtime, "reply with the single word two")?;
        println!("queued at places {first} and {second}");
    } else {
        session.type_line(&options.prompt)?;
    }
    let began = Instant::now();
    let mut trace: Vec<String> = Vec::new();
    let mut busy_saved = false;
    let mut after: Option<(String, Instant)> = None;
    let mut settled = false;
    while began.elapsed() < options.wait {
        std::thread::sleep(POLL);
        let Some(now) = session.screen() else {
            continue;
        };
        if trace.last() != Some(&now) {
            save(
                out,
                &format!("{runtime}-trace/{:02}.txt", trace.len()),
                &now,
            )?;
            trace.push(now.clone());
            after = Some((now.clone(), Instant::now()));
        } else if let Some((screen, since)) = &after {
            if since.elapsed() >= Duration::from_secs(4) && trace.len() > 2 {
                save(out, &format!("{runtime}-after-turn.txt"), screen)?;
                settled = true;
                break;
            }
        }
        if !busy_saved && began.elapsed() >= Duration::from_secs(1) {
            save(out, &format!("{runtime}-busy.txt"), &now)?;
            busy_saved = true;
        }
    }
    if !settled {
        println!(
            "the turn had not settled after {:?}; the last screen is kept",
            options.wait
        );
        if let Some(last) = trace.last() {
            save(out, &format!("{runtime}-after-turn.txt"), last)?;
        }
    }
    let ledger = home.path().join(".karen/logs/karen-runtime.jsonl");
    if let Ok(filed) = std::fs::read_to_string(ledger) {
        println!("what the host filed:\n{filed}");
    }
    println!(
        "{} distinct screens during the turn; read them before keeping any",
        trace.len()
    );
    Ok(())
}
