//! Karen — the Karen you call when the agent ignores your rules.
//! She enforces YOUR rules before the harness even blinks.
//! External hooks use the `karen` binary; in-process runtimes link
//! this crate and call `Engine::dispatch`. The Karen who links.

pub mod adapters;
pub mod engine;
pub mod policy;
pub mod state;
pub mod types;

pub use adapters::{Claude, ClaudeClassicAdapter, RuntimeAdapter};
pub use engine::Engine;
pub use policy::{PolicySet, load, locate_policies};
pub use types::{Action, AdapterType, EngineRequest, EngineResponse, Event};
