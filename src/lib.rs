#![deny(warnings)]

pub mod dbus;
pub mod error;
pub mod markdown;
pub mod model;
pub mod operations;
pub mod service;
pub mod storage;
pub mod tools;

use mcp_core::ServerConfig;

/// The MCP server configuration handed to `mcp-core`. Stdio only (plus the
/// separate D-Bus surface).
///
/// MF-12: the websocket transport is refused — mcp-core's ws transport is
/// unauthenticated, so `serve --transport websocket --host 0.0.0.0` would
/// expose every task read/write to anyone who can reach the port. tasks-mcp
/// is stdio-served (and D-Bus-served) in practice.
///
/// The `instructions` string is emitted in the MCP `initialize` response and is
/// what the orchestrator indexes as this server's model-facing description, so
/// it doubles as the server-level tool-discovery hint (what this is / when to
/// reach for it).
pub fn server_config() -> ServerConfig {
    ServerConfig::new("tasks-mcp", env!("CARGO_PKG_VERSION")).without_websocket()
}
