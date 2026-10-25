// tests/e2e/mod.rs
// The Karen who orchestrates e2e tests because someone has to herd the cats.

#[cfg(test)]
mod adapters;
#[cfg(test)]
mod cli;
#[cfg(test)]
mod common;
#[cfg(test)]
mod examples_pack;
#[cfg(test)]
mod hook;
#[cfg(test)]
mod policy;
#[cfg(test)]
mod runtime_integration;
#[cfg(test)]
mod state;
