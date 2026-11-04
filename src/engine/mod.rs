//! The engine she runs — every tool call, every verdict, every lecture. The Karen who runs this establishment, and has never once been asked to.

pub mod dispatcher;
pub mod facts;
pub mod injector;
pub mod jsonl;
pub mod maintenance;
pub mod runtime_command;
pub mod watch;

pub use dispatcher::Engine;
pub use watch::{WatchError, WatchGuard};
