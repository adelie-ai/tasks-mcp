#![deny(warnings)]

use std::sync::Arc;

use clap::Parser;
use mcp_core::{CommonServeArgs, ServerCore};
use tasks_mcp::dbus::DbusStartupError;
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

    // TODO(mcp-core#40, red commit): telemetry is not installed yet, so no
    // subscriber exists and every `tracing` call in this process is a no-op.

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
                        log_dbus_startup_failure(&e);
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

/// Log a failed D-Bus service startup at the level rule 8.2 calls for.
///
/// A missing session bus is an absent optional capability under the
/// platform's capability-based degradation rule (`AGENTS.md`) — expected on a
/// headless box or in a container, not a fault, so it stays at `warn!`.
/// Failing to claim the bus name, or any other setup failure, is a genuine
/// failure an operator needs to see, so it goes to `error!`.
fn log_dbus_startup_failure(err: &DbusStartupError) {
    // TODO(mcp-core#40, red commit): everything logs at ERROR regardless of
    // whether the condition was expected.
    match err {
        DbusStartupError::NoSessionBus(_) => {
            tracing::error!(error = %err, "D-Bus service failed to start");
        }
        DbusStartupError::NameTaken(_) | DbusStartupError::SetupFailed(_) => {
            tracing::error!(error = %err, "D-Bus service failed to start");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use tracing::span::{Attributes, Id, Record};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::registry::LookupSpan;

    use super::*;

    // Local copy of the capturing layer (see `tasks_mcp::dbus`'s test module
    // for the fuller version with span capture) — this file only needs event
    // levels.
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<tracing::Level>>>);

    impl<S> Layer<S> for Capture
    where
        S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_new_span(&self, _attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {}
        fn on_record(&self, _id: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {}
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            self.0
                .lock()
                .expect("capture lock")
                .push(*event.metadata().level());
        }
    }

    fn levels_logged(err: &DbusStartupError) -> Vec<tracing::Level> {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        tracing::subscriber::with_default(subscriber, || {
            log_dbus_startup_failure(err);
        });
        capture.0.lock().expect("capture lock").clone()
    }

    #[test]
    fn no_session_bus_logs_at_warn_not_error() {
        let err = DbusStartupError::NoSessionBus(zbus::Error::InputOutput(Arc::new(
            io::Error::new(io::ErrorKind::NotFound, "no such file or directory (test)"),
        )));
        let levels = levels_logged(&err);
        assert_eq!(
            levels,
            vec![tracing::Level::WARN],
            "an absent session bus must log at WARN, not ERROR: {levels:?}"
        );
    }

    #[test]
    fn name_taken_logs_at_error() {
        let err = DbusStartupError::NameTaken(zbus::Error::NameTaken);
        let levels = levels_logged(&err);
        assert_eq!(
            levels,
            vec![tracing::Level::ERROR],
            "a name conflict is a genuine failure and must log at ERROR: {levels:?}"
        );
    }

    #[test]
    fn setup_failed_logs_at_error() {
        let err = DbusStartupError::SetupFailed(zbus::Error::Handshake(
            "test: handshake rejected".to_string(),
        ));
        let levels = levels_logged(&err);
        assert_eq!(
            levels,
            vec![tracing::Level::ERROR],
            "an unclassified setup failure must log at ERROR: {levels:?}"
        );
    }
}
