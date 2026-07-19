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

pub use crate::service::TasksService;

use crate::error::Result;
use crate::storage::Storage;

/// Construct the [`TasksService`] with built-in defaults for in-process hosting
/// (da#538 Phase C).
///
/// This is the single default construction path for the MCP service: the
/// standalone `serve` binary routes through it, and an in-process host (the
/// daemon compiling this server in) can call it with zero configuration.
///
/// Why: the store root is resolved by [`Storage::new`] from the `TASKS_MCP_ROOT`
/// environment variable, falling back to the default local data directory when
/// unset - the same root the binary uses with no extra flags. The task
/// directory is created lazily on first use, so construction has no filesystem
/// side effect.
pub fn build_service() -> Result<TasksService> {
    let storage = Storage::new()?;
    Ok(TasksService::new(storage))
}

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
    ServerConfig::new("tasks-mcp", env!("CARGO_PKG_VERSION"))
        .without_websocket()
        .instructions(
            "Local task and project tracker: captures to-dos as Markdown files (with YAML \
             frontmatter) under the user's local data directory, organized into named lists \
             with a two-level epic -> deliverable hierarchy. Reach for it whenever the user \
             wants to capture, find, or update work items - adding a task, checking what is on \
             their list, marking something done, tracking what they are working on, or planning \
             a project. Discover with list_lists and list_tasks / search_tasks, then use \
             create_task, set_status, update_task, and append_task_note to record progress; \
             every task is addressed by id or path and moves through the statuses todo, doing, \
             blocked, validating, done, and canceled. It operates purely on local files with no \
             external accounts or network - add_external_ref only records a link to an outside \
             ticket (e.g. Jira or GitHub) and does not sync with those systems.",
        )
}

#[cfg(test)]
mod tests {
    use mcp_core::McpService;

    use super::{build_service, server_config};
    use crate::storage::Storage;

    /// `build_service()` constructs a usable service with no configuration, so an
    /// in-process host can stand the server up with zero setup (da#538 Phase C).
    #[test]
    fn build_service_constructs_service_with_no_config() {
        let service = build_service().expect("build_service must succeed with built-in defaults");
        assert!(
            !service.tools().is_empty(),
            "the built-in service must expose its task tools"
        );
    }

    /// The zero-config constructor must resolve the same default store root the
    /// standalone binary uses (`TASKS_MCP_ROOT` or the default data dir), i.e. it
    /// routes through the same `Storage::new` default resolution.
    #[test]
    fn build_service_uses_default_storage_root() {
        let service = build_service().expect("build_service must succeed with built-in defaults");
        let expected = Storage::new().expect("Storage::new resolves the default store root");
        assert_eq!(
            service.storage().root(),
            expected.root(),
            "build_service must resolve the same default store root as Storage::new"
        );
    }

    /// The server must advertise a non-empty `instructions` blurb so the
    /// orchestrator has a server-level description to index for tool discovery.
    #[test]
    fn server_config_exposes_non_empty_instructions() {
        let instructions = server_config()
            .instructions
            .expect("server_config() must set MCP instructions");
        assert!(
            !instructions.trim().is_empty(),
            "instructions must not be empty or whitespace"
        );
    }

    /// The blurb must convey the task-tracking purpose and name the core
    /// discovery and write tools so it aids server- and tool-level selection.
    #[test]
    fn instructions_mentions_key_tools_and_purpose() {
        let instructions = server_config()
            .instructions
            .expect("server_config() must set MCP instructions")
            .to_lowercase();
        assert!(
            instructions.contains("task"),
            "instructions should describe task tracking"
        );
        for tool in ["list_tasks", "search_tasks", "create_task", "set_status"] {
            assert!(
                instructions.contains(tool),
                "instructions should name the `{tool}` tool for discovery"
            );
        }
    }
}
