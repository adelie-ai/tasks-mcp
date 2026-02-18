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
    long_about = "tasks-mcp provides task storage and management over MCP for LLM orchestrators.\n\nUsage:\n  tasks-mcp serve --mode stdio\n  tasks-mcp serve --mode websocket --host 0.0.0.0 --port 8080"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Serve {
        #[arg(short, long, default_value_t = TransportMode::Stdio)]
        mode: TransportMode,
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { mode, host, port } => {
            let server = McpServer::new();
            match mode {
                TransportMode::Stdio => tasks_mcp::transport::run_stdio_server(server).await?,
                TransportMode::Websocket => {
                    tasks_mcp::transport::run_websocket_server(server, &host, port).await?
                }
            }
        }
    }

    Ok(())
}
