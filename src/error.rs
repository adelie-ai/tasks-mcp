#![deny(warnings)]

use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, TaskMcpError>;

/// Serialize a value we are about to return as a tool result.
///
/// Use this instead of `serde_json::to_value(x)?`: a bare `?` would convert
/// into [`TaskMcpError::ArgumentJson`] and report our own bug as the caller's
/// bad input.
pub fn serialize_result<T: serde::Serialize>(value: &T) -> Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|e| TaskMcpError::ResultSerialization(e.to_string()))
}

#[derive(Debug, Error)]
pub enum TaskMcpError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// Failed to deserialize the **caller's** arguments.
    ///
    /// This carries the blanket `From<serde_json::Error>`, so a bare `?` on a
    /// `serde_json::from_value`/`from_str` of tool arguments lands here — which
    /// is what almost every `?` in this crate is doing. It maps to
    /// `CallError::InvalidParams`, which mcp-core shows to the model as
    /// `isError` content it can correct.
    ///
    /// **Do not reach this by `?`-ing a serialization of our own reply.** That
    /// is a server fault and belongs in [`Self::ResultSerialization`]; use
    /// [`serialize_result`] rather than `?`. The variant is named for its origin
    /// precisely so a mis-fit is visible at the call site.
    #[error("invalid arguments: {0}")]
    ArgumentJson(#[from] serde_json::Error),

    /// Failed to serialize **our own** reply — a bug on this side, not something
    /// the caller supplied. Maps to `CallError::Internal`, so it stays a
    /// protocol error rather than telling the model to fix arguments that were
    /// already correct.
    #[error("failed to serialize result: {0}")]
    ResultSerialization(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid task document: {0}")]
    InvalidTaskDocument(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}
