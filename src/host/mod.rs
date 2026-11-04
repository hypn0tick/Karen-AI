//! The terminal host: `karen run <runtime>`. The Karen who holds the terminal, so she can type when
//! the hook API can't.
//!
//! Claude Code and Codex cannot be handed a slash command through their hooks, and there is no
//! output field that does it. A command reaches them the way yours does: typed into their terminal.
//! `karen run` starts the runtime in a pseudo-terminal Karen owns (`ConPTY` on Windows, a pty
//! elsewhere), keeps a `vt100` copy of what it draws, and serves a local control endpoint that the
//! runtime's own hook processes find through the `KAREN_HOST_*` variables they inherit.
//!
//! This module is the default-on `host` feature. `--no-default-features` is the lean build with
//! hooks and gates and none of this.

pub mod console;
pub mod endpoint;
pub mod program;
pub mod session;
pub mod terminal;

pub use endpoint::{Endpoint, EndpointError, Request, Response};
pub use session::{RunError, RunOptions, run};
