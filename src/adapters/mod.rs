//! Adapters: she speaks their dialect so you don't have to — every runtime made up its own word for "no", and she's fluent in all of them. The Karen who translates.

mod claude_classic;

pub use claude_classic::ClaudeClassic;

/// Claude Code runtime with modifiedInput support. The Karen who stays current, and asks which version you're on.
#[derive(Debug, Default)]
pub struct Claude;

impl RuntimeAdapter for Claude {
    fn kind(&self) -> AdapterType {
        AdapterType::Claude
    }

    fn normalize(&self, raw: serde_json::Value) -> Result<EngineRequest, AdapterError> {
        // Same normalization as Classic — she updates her address, not her personality.
        ClaudeClassic.normalize(raw)
    }

    fn denormalize(&self, request: &EngineRequest, response: EngineResponse) -> serde_json::Value {
        ClaudeClassic.denormalize(request, response)
    }
}

/// Claude Code runtime without modifiedInput support: verdicts only, delivered with eye contact. The Karen who tells you what's wrong and waits, arms crossed, without touching your things.
#[derive(Debug, Default)]
pub struct ClaudeClassicAdapter;

impl RuntimeAdapter for ClaudeClassicAdapter {
    fn kind(&self) -> AdapterType {
        AdapterType::ClaudeClassic
    }

    fn normalize(&self, raw: serde_json::Value) -> Result<EngineRequest, AdapterError> {
        // Same normalization as current — fluent, and with an accent she'll deny having.
        ClaudeClassic.normalize(raw)
    }

    fn denormalize(&self, request: &EngineRequest, response: EngineResponse) -> serde_json::Value {
        ClaudeClassic.denormalize(request, response)
    }
}

use crate::types::{AdapterType, EngineRequest, EngineResponse};

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("invalid JSON payload: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported hook event `{0}`")]
    UnknownEvent(String),
    #[error("payload is not a JSON object")]
    NotObject,
}

/// Translates a runtime's hook protocol to and from the engine types.
pub trait RuntimeAdapter {
    fn kind(&self) -> AdapterType;

    /// Normalize a raw runtime payload into an engine request.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::NotObject`] when the payload is not a JSON
    /// object, and [`AdapterError::UnknownEvent`] when it names an event
    /// she does not recognize — the Karen who refuses to guess what a
    /// malformed payload "probably meant".
    fn normalize(&self, raw: serde_json::Value) -> Result<EngineRequest, AdapterError>;
    fn denormalize(&self, request: &EngineRequest, response: EngineResponse) -> serde_json::Value;
}
