#![deny(warnings)]

use clap::{Parser, ValueEnum};
use std::fmt;
use tasks_mcp::error::Result;
use tasks_mcp::server::McpServer;

#[derive(Clone, Debug, ValueEnum)]
enum TransportMode {
    Stdio,
    Websocket,
}

impl fmt::Display for TransportMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportMode::Stdio => write!(f, "stdio"),
            TransportMode::Websocket => write!(f, "websocket"),
        }
    }
}

#[derive(Parser)]
#[command(name = "tasks-mcp")]
#[command(about = "Tasks MCP Server")]
#[command(
    long_about = "tasks-mcp provides task storage and management over MCP for LLM orchestrators.\n\nUsage:\n  tasks-mcp serve --mode stdio\n  tasks-mcp serve --mode websocket --host 0.0.0.0 --port 8080\n  tasks-mcp dbus       # D-Bus only (used by D-Bus activation)"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Run an MCP transport (stdio or websocket) with the D-Bus service also active.
    Serve {
        #[arg(short, long, default_value_t = TransportMode::Stdio)]
        mode: TransportMode,
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
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
        Commands::Serve {
            mode,
            host,
            port,
            no_dbus,
        } => {
            let server = McpServer::new();
            if !no_dbus {
                let storage = server.storage().clone();
                tokio::spawn(async move {
                    if let Err(e) = tasks_mcp::dbus::run_dbus_service(storage).await {
                        eprintln!("D-Bus service error: {e}");
                    }
                });
            }
            match mode {
                TransportMode::Stdio => tasks_mcp::transport::run_stdio_server(server).await?,
                TransportMode::Websocket => {
                    tasks_mcp::transport::run_websocket_server(server, &host, port).await?
                }
            }
        }
        Commands::Dbus => {
            let server = McpServer::new();
            server.storage().ensure_root().await?;
            tasks_mcp::dbus::run_dbus_service(server.storage().clone()).await?;
        }
    }

    Ok(())
}
