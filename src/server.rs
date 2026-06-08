#![deny(warnings)]

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::RwLock;

use crate::error::Result;
use crate::storage::Storage;

/// Per-connection initialized state.
///
/// The MCP `initialize` / `notifications/initialized` handshake is
/// per-connection.  Each transport connection (stdio session, WebSocket
/// connection) should hold its own `ConnectionState` so that multiple
/// concurrent WebSocket clients do not share initialization flags.
pub type ConnectionState = Arc<RwLock<bool>>;

pub fn new_connection_state() -> ConnectionState {
    Arc::new(RwLock::new(false))
}

#[derive(Debug, Clone)]
pub struct McpServer {
    storage: Storage,
}

impl McpServer {
    pub fn new() -> Self {
        let storage = Storage::new().expect("storage initialization must not fail");
        Self { storage }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub async fn handle_initialize(
        &self,
        protocol_version: &str,
        _client_capabilities: &Value,
    ) -> Result<Value> {
        self.storage.ensure_root().await?;
        Ok(json!({
            "protocolVersion": protocol_version,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "tasks-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    pub fn list_tools(&self) -> Vec<Value> {
        crate::tools::tool_definitions()
    }

    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        crate::tools::call_tool(self, tool_name, arguments).await
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}
