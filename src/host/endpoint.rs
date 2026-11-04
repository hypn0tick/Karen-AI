//! The control endpoint: how a short-lived `karen hook` (or `karen deliver`) process asks the
//! long-lived `karen run` process about its screen, or has it type a line. The Karen who answers the
//! door, but only to people who know the password.
//!
//! One connection is one exchange. The client sends two lines, the token and then a JSON request,
//! and reads one JSON line back. The token is the `KAREN_HOST_TOKEN` the hosted runtime inherited
//! from `karen run`, so anything that is that runtime's descendant can use the endpoint and nothing
//! else can: on Windows the named pipe's default access also keeps other users from writing to it,
//! and on Unix the socket sits in a directory only its owner can enter.
//!
//! What the token is not: a boundary against the hosted tree itself. Everything running under the
//! runtime shares its terminal and could type into it without Karen. See `docs/runtime-control.md`.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};
use serde::{Deserialize, Serialize};

use super::terminal::ScreenDump;

/// The endpoint's name (a pipe name, or a socket path), exported to the hosted runtime.
pub const ENV_ENDPOINT: &str = "KAREN_HOST_ENDPOINT";
/// The password for the endpoint.
pub const ENV_TOKEN: &str = "KAREN_HOST_TOKEN";
/// The runtime being hosted (`claude`, `codex`, `opencode`, or the program's own name).
pub const ENV_RUNTIME: &str = "KAREN_HOST_RUNTIME";
/// The pid of the `karen run` process.
pub const ENV_PID: &str = "KAREN_HOST_PID";

/// The longest token line she will read. A real one is 32 characters.
const MAX_TOKEN_LINE: u64 = 256;
/// The longest request line she will read.
const MAX_REQUEST_LINE: u64 = 16 * 1024;
/// How many connections she serves at once. A client that connects and says nothing holds one.
const MAX_CONNECTIONS: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum EndpointError {
    #[error("cannot create the control endpoint: {0}")]
    Bind(std::io::Error),
    #[error("cannot reach the control endpoint: {0}")]
    Connect(std::io::Error),
    #[error("the control endpoint spoke nonsense: {0}")]
    Protocol(String),
    #[error("cannot make a token: {0}")]
    Token(getrandom::Error),
}

/// What a client may ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Who is hosted, how big the terminal is, how long it has been quiet.
    Status,
    /// The visible screen as plain text.
    Screen,
    /// Type one line of printable text and press Enter, now, whatever the runtime is doing.
    Type { text: String },
}

/// What the host reports about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostStatus {
    pub runtime: String,
    pub host_pid: u32,
    pub child_pid: Option<u32>,
    pub rows: u16,
    pub cols: u16,
    /// Milliseconds since the runtime last drew anything.
    pub quiet_ms: u64,
}

/// The one-line answer to a [`Request`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<HostStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenDump>,
    /// How many characters were typed (not counting the Enter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed: Option<usize>,
}

impl Response {
    #[must_use]
    pub fn refused(why: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(why.into()),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn ok() -> Self {
        Self {
            ok: true,
            ..Self::default()
        }
    }
}

/// Whoever answers requests on behalf of the host.
pub trait Handler: Send + Sync {
    fn handle(&self, request: &Request) -> Response;
}

/// Where the endpoint lives and the password for it.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub name: String,
    pub token: String,
    /// The directory holding a Unix socket, removed by [`Endpoint::close`].
    directory: Option<PathBuf>,
}

impl Endpoint {
    /// Remove whatever the endpoint left on disk. A named pipe leaves nothing.
    pub fn close(&self) {
        if let Some(directory) = &self.directory {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

fn random_hex(bytes: usize) -> Result<String, EndpointError> {
    let mut raw = vec![0_u8; bytes];
    getrandom::fill(&mut raw).map_err(EndpointError::Token)?;
    Ok(raw
        .iter()
        .fold(String::with_capacity(bytes * 2), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
}

/// Compare without stopping at the first difference, so the time taken says nothing about how much
/// of a guess was right.
fn same_token(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for (x, y) in a.iter().zip(b) {
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

#[cfg(windows)]
fn local_name(name: &str) -> std::io::Result<interprocess::local_socket::Name<'_>> {
    use interprocess::local_socket::GenericNamespaced;
    name.to_ns_name::<GenericNamespaced>()
}

#[cfg(not(windows))]
fn local_name(name: &str) -> std::io::Result<interprocess::local_socket::Name<'_>> {
    use interprocess::local_socket::GenericFilePath;
    name.to_fs_name::<GenericFilePath>()
}

/// Where this host's endpoint goes, as the string a client connects to, and the directory to remove
/// afterwards (Unix only).
#[cfg_attr(windows, allow(clippy::unnecessary_wraps))]
fn place(id: &str) -> Result<(String, Option<PathBuf>), EndpointError> {
    #[cfg(windows)]
    {
        Ok((format!("karen-host-{id}"), None))
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::DirBuilderExt;
        let directory = std::env::temp_dir().join(format!("karen-host-{id}"));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(EndpointError::Bind)?;
        let socket = directory.join("s");
        Ok((socket.to_string_lossy().into_owned(), Some(directory)))
    }
}

/// Create the endpoint and its password. Nothing is served until [`serve`] is called.
///
/// # Errors
///
/// When no random token can be made or the pipe or socket cannot be created.
pub fn bind() -> Result<(interprocess::local_socket::Listener, Endpoint), EndpointError> {
    let id = random_hex(8)?;
    let token = random_hex(16)?;
    let (name, directory) = place(&id)?;
    let listener = ListenerOptions::new()
        .name(local_name(&name).map_err(EndpointError::Bind)?)
        .create_sync()
        .map_err(EndpointError::Bind)?;
    Ok((
        listener,
        Endpoint {
            name,
            token,
            directory,
        },
    ))
}

/// Answer connections on `listener` for as long as the process lives. Runs on its own threads and
/// never returns an error to the host: a bad client is that client's problem.
pub fn serve(
    listener: interprocess::local_socket::Listener,
    token: String,
    handler: Arc<dyn Handler>,
) {
    let live = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(stream) = connection else {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            };
            if live.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                live.fetch_sub(1, Ordering::SeqCst);
                continue;
            }
            let (token, handler, live) = (token.clone(), Arc::clone(&handler), Arc::clone(&live));
            std::thread::spawn(move || {
                let _ = exchange(stream, &token, handler.as_ref());
                live.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
}

/// Read a line of at most `limit` bytes, without its newline.
fn read_line(reader: &mut BufReader<Stream>, limit: u64) -> std::io::Result<String> {
    let mut line = String::new();
    reader.by_ref().take(limit).read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn exchange(stream: Stream, token: &str, handler: &dyn Handler) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    let offered = read_line(&mut reader, MAX_TOKEN_LINE)?;
    let response = if same_token(&offered, token) {
        let request = read_line(&mut reader, MAX_REQUEST_LINE)?;
        match serde_json::from_str::<Request>(&request) {
            Ok(request) => handler.handle(&request),
            Err(why) => Response::refused(format!("not a request she knows: {why}")),
        }
    } else {
        Response::refused("wrong token")
    };
    let mut line = serde_json::to_string(&response).map_err(std::io::Error::other)?;
    line.push('\n');
    let stream = reader.get_mut();
    stream.write_all(line.as_bytes())?;
    stream.flush()
}

/// Ask the host at `name` one question, with its `token`.
///
/// # Errors
///
/// When the endpoint cannot be reached or does not answer in the protocol. A refusal is a
/// [`Response`] with `ok` false, not an error.
pub fn call(name: &str, token: &str, request: &Request) -> Result<Response, EndpointError> {
    let stream = Stream::connect(local_name(name).map_err(EndpointError::Connect)?)
        .map_err(EndpointError::Connect)?;
    let mut reader = BufReader::new(stream);
    let body =
        serde_json::to_string(request).map_err(|why| EndpointError::Protocol(why.to_string()))?;
    reader
        .get_mut()
        .write_all(format!("{token}\n{body}\n").as_bytes())
        .map_err(EndpointError::Connect)?;
    reader.get_mut().flush().map_err(EndpointError::Connect)?;
    let answer = read_line(&mut reader, 4 * 1024 * 1024).map_err(EndpointError::Connect)?;
    serde_json::from_str(&answer).map_err(|why| EndpointError::Protocol(why.to_string()))
}

/// The endpoint a hosted runtime was told about: `(name, token)` from `KAREN_HOST_*`, or `None`
/// when this process is not running under `karen run`.
#[must_use]
pub fn from_env() -> Option<(String, String)> {
    let name = std::env::var(ENV_ENDPOINT).ok().filter(|v| !v.is_empty())?;
    let token = std::env::var(ENV_TOKEN).ok().filter(|v| !v.is_empty())?;
    Some((name, token))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;
    impl Handler for Echo {
        fn handle(&self, request: &Request) -> Response {
            match request {
                Request::Type { text } => Response {
                    typed: Some(text.chars().count()),
                    ..Response::ok()
                },
                _ => Response::ok(),
            }
        }
    }

    fn served() -> Endpoint {
        let (listener, endpoint) = bind().unwrap();
        serve(listener, endpoint.token.clone(), Arc::new(Echo));
        endpoint
    }

    #[test]
    fn endpoint_answers_a_client_with_the_token() {
        let endpoint = served();
        let answer = call(
            &endpoint.name,
            &endpoint.token,
            &Request::Type {
                text: "héllo".into(),
            },
        )
        .unwrap();
        assert!(answer.ok);
        assert_eq!(answer.typed, Some(5), "characters, not bytes");
        endpoint.close();
    }

    #[test]
    fn endpoint_refuses_a_wrong_token_and_says_nothing_else() {
        let endpoint = served();
        let answer = call(&endpoint.name, "not-the-token", &Request::Status).unwrap();
        assert_eq!(answer, Response::refused("wrong token"));
        endpoint.close();
    }

    #[test]
    fn endpoint_refuses_a_request_it_cannot_read() {
        let endpoint = served();
        let stream = Stream::connect(local_name(&endpoint.name).unwrap()).unwrap();
        let mut reader = BufReader::new(stream);
        let hello = format!("{}\n{{\"op\":\"rm -rf\"}}\n", endpoint.token);
        reader.get_mut().write_all(hello.as_bytes()).unwrap();
        let answer: Response =
            serde_json::from_str(&read_line(&mut reader, 4096).unwrap()).unwrap();
        assert!(!answer.ok);
        assert!(answer.error.unwrap().contains("not a request she knows"));
        endpoint.close();
    }

    #[test]
    fn endpoint_names_differ_per_host() {
        let (_a, first) = bind().unwrap();
        let (_b, second) = bind().unwrap();
        assert_ne!(first.name, second.name);
        assert_ne!(first.token, second.token);
        assert_eq!(first.token.len(), 32);
        first.close();
        second.close();
    }

    #[test]
    fn token_comparison_rejects_prefixes_and_extensions() {
        assert!(same_token("abc", "abc"));
        assert!(!same_token("abc", "abd"));
        assert!(!same_token("abc", "ab"));
        assert!(!same_token("ab", "abc"));
        assert!(!same_token("", "abc"));
    }
}
