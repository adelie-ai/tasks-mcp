#![deny(warnings)]

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::RwLock;

use crate::error::Result;
use crate::storage::Storage;

#[derive(Debug, Clone)]
pub struct McpServer {
    initialized: Arc<RwLock<bool>>,
    storage: Storage,
}

impl McpServer {
    pub fn new() -> Self {
        let storage = Storage::new().expect("storage initialization must not fail");
        Self {
            initialized: Arc::new(RwLock::new(false)),
            storage,
        }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub async fn is_initialized(&self) -> bool {
        *self.initialized.read().await
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

    pub async fn handle_initialized(&self) -> Result<()> {
        let mut guard = self.initialized.write().await;
        *guard = true;
        Ok(())
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
