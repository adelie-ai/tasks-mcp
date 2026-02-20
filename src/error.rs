#![deny(warnings)]

use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, TaskMcpError>;

#[derive(Debug, Error)]
pub enum TaskMcpError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

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

    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("invalid message: {0}")]
    InvalidMessage(String),

    #[error("connection closed")]
    ConnectionClosed,
}
