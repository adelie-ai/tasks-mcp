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
/// The split is *can the caller do anything about it*. Bad arguments and every
/// domain failure become `isError` content the model can read and correct
/// (SEP-1303); only a fault on our own side stays a protocol error.
///
/// Why the two JSON variants matter: a failure serializing our own reply is a
/// bug here, not bad input. Reporting it as invalid params tells the model to
/// rewrite arguments that were already correct, and since SEP-1303 that message
/// is shown to the model rather than merely logged.
fn to_call_error(err: TaskMcpError) -> CallError {
    match err {
        TaskMcpError::ArgumentJson(_) => CallError::invalid_params(err.to_string()),
        TaskMcpError::ResultSerialization(_) => CallError::internal(err.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Bad model-supplied arguments must stay `InvalidParams`, which mcp-core
    /// surfaces as `isError` content the model can read and correct (SEP-1303).
    #[test]
    fn bad_tool_arguments_map_to_invalid_params() {
        let err = serde_json::from_str::<crate::operations::task_ops::TaskLocator>("{\"id\": 42}")
            .expect_err("a numeric id must fail to deserialize");
        assert!(
            matches!(
                to_call_error(TaskMcpError::from(err)),
                CallError::InvalidParams(_)
            ),
            "argument deserialization is the model's fault"
        );
    }

    /// A failure serializing *our own* reply is a server fault. Reporting it as
    /// invalid params tells the model to rewrite arguments that were fine, and
    /// since SEP-1303 that message is shown to the model rather than merely
    /// logged — so the misclassification actively wastes turns.
    #[test]
    fn reply_serialization_failure_maps_to_internal() {
        // A non-string map key is one of the few things serde_json genuinely
        // refuses to serialize, so this exercises the real helper rather than a
        // hand-built variant.
        let mut unserializable = std::collections::BTreeMap::new();
        unserializable.insert((1u8, 2u8), 3u8);
        let err = crate::error::serialize_result(&unserializable)
            .expect_err("a non-string map key cannot serialize to JSON");
        assert!(
            matches!(err, TaskMcpError::ResultSerialization(_)),
            "serialize_result must not classify our own fault as bad arguments: {err:?}"
        );
        assert!(
            matches!(to_call_error(err), CallError::Internal(_)),
            "our own serialization fault is not something the model can fix"
        );
    }

    /// Every other domain error stays a tool-level failure.
    #[test]
    fn domain_errors_stay_tool_errors() {
        for err in [
            TaskMcpError::NotFound("task-1".into()),
            TaskMcpError::Conflict("already exists".into()),
            TaskMcpError::InvalidArgument("bad".into()),
        ] {
            assert!(matches!(to_call_error(err), CallError::Tool(_)));
        }
    }
}
