//! Policies you write, she loads — and enforces to the letter, because you're the one who wrote the letter. The Karen who loads your rules like ammunition.

pub mod gate;
pub mod loader;

pub use gate::{ActionDef, Gate, GateDef};
pub use loader::{Config, LoadError, MaintenanceConfig, PolicySet, Rule, load, locate_policies};
