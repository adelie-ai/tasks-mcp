#![deny(warnings)]

use std::sync::Arc;

use clap::Parser;
use mcp_core::{CommonServeArgs, ServerCore};
use tasks_mcp::error::{Result, TaskMcpError};
use tasks_mcp::server_config;
use tasks_mcp::storage::Storage;

#[derive(Parser)]
#[command(name = "tasks-mcp")]
#[command(about = "Tasks MCP Server")]
#[command(
    long_about = "tasks-mcp provides task storage and management over MCP for LLM orchestrators.\n\nUsage:\n  tasks-mcp serve --mode stdio\n  tasks-mcp dbus       # D-Bus only (used by D-Bus activation)"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Run an MCP transport (stdio/unix) with the D-Bus service also active.
    Serve {
        /// Transport-selection flags (`--transport`/`--mode`, `--host`, `--port`, `--socket-path`).
        #[command(flatten)]
        common: CommonServeArgs,
        /// Disable the automatic D-Bus service alongside this transport.
        #[arg(long)]
        no_dbus: bool,
    },
    /// Run the D-Bus service only (used by D-Bus activation files).
    Dbus,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { common, no_dbus } => {
            // The MCP service is built through the shared zero-config
            // constructor so the binary and in-process hosts share one default
            // construction path (da#538). One Storage handle backs both the MCP
            // service and the D-Bus service, so both surfaces see identical task
            // data; the per-connection `initialized` handshake state lives in
            // mcp-core's `Session`.
            let service = tasks_mcp::build_service()?;
            let storage = service.storage().clone();
            storage.ensure_root().await?;

            let core = ServerCore::new(server_config(), Arc::new(service));

            // Run the D-Bus service concurrently with the MCP transport. A
            // D-Bus failure (e.g. no session bus available) is logged but does
            // not tear down the MCP server — the MCP transport drives process
            // lifetime, exiting on EOF/shutdown as before.
            if !no_dbus {
                tokio::spawn(async move {
                    if let Err(e) = tasks_mcp::dbus::run_dbus_service(storage).await {
                        eprintln!("D-Bus service error: {e}");
                    }
                });
            }

            mcp_core::serve(core, &common)
                .await
                .map_err(|e| TaskMcpError::Internal(e.to_string()))?;
        }
        Commands::Dbus => {
            let storage = Storage::new()?;
            storage.ensure_root().await?;
            tasks_mcp::dbus::run_dbus_service(storage).await?;
        }
    }

    Ok(())
}
