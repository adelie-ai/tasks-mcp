//! The [`McpService`] implementation tasks-mcp hands to `mcp-core`.
//!
//! `mcp-core` owns the JSON-RPC protocol, framing, transports, and CLI; this
//! module only describes the task tools and executes them against the shared
//! [`Storage`]. The same `Storage` handle is shared with the D-Bus service so
//! both surfaces see identical data.

#![deny(warnings)]

use mcp_core::{CallError, McpService, ToolDef, ToolReply, async_trait};
use serde_json::Value;

use crate::error::TaskMcpError;
use crate::storage::Storage;
use crate::tools;

/// MCP service over the shared task [`Storage`].
#[derive(Clone)]
pub struct TasksService {
    storage: Storage,
}

impl TasksService {
    /// Build a service over the given storage handle.
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    /// The storage handle, shared with the D-Bus service.
    pub fn storage(&self) -> &Storage {
        &self.storage
    }
}

/// Map a domain error to the appropriate [`CallError`].
///
/// Argument (de)serialization failures are genuine protocol faults
/// (`-32602`); everything else is a tool-level failure the model should see
/// and react to (surfaced by the core as `isError: true` content).
fn to_call_error(err: TaskMcpError) -> CallError {
    match err {
        TaskMcpError::Json(_) => CallError::invalid_params(err.to_string()),
        other => CallError::tool(other.to_string()),
    }
}

#[async_trait]
impl McpService for TasksService {
    fn tools(&self) -> Vec<ToolDef> {
        tools::tool_definitions()
    }

    async fn call_tool(&self, name: &str, arguments: &Value) -> Result<ToolReply, CallError> {
        let value = tools::call_tool(&self.storage, name, arguments.clone())
            .await
            .map_err(to_call_error)?;
        Ok(ToolReply::json(&value)?)
    }
}
