//! D-Bus service interface for tasks-mcp.
//!
//! Service name : `org.tasks.TasksMcp`
//! Object path  : `/org/tasks/TasksMcp`
//! Interface    : `org.tasks.TasksMcp`
//!
//! Complex arguments and return values are JSON strings, matching the shape
//! that the MCP tool layer already uses.  Callers on the D-Bus side can
//! therefore reuse the same JSON schemas documented for the MCP tools.
//!
//! Write operations emit the `TasksChanged` signal after each successful
//! mutation so that QML widgets (or any other subscriber) can refresh.
//!
//! # Telemetry
//!
//! D-Bus calls do not pass through `mcp-core`'s JSON-RPC dispatcher, so they
//! are invisible to the span, counter and histogram it records for `tools/call`
//! (adelie-ai/mcp-core#40). [`record_dbus_call`] mirrors that instrumentation
//! for this surface: a `tasks_mcp.dbus.call` span and matching metrics, keyed
//! only by the fixed, compile-time operation name — never by an argument.

#![deny(warnings)]

use std::future::Future;
use std::time::Instant;

use mcp_core::telemetry::metrics::{self, Label};
use serde_json::json;
use thiserror::Error;
use tracing::Instrument;
use zbus::object_server::SignalEmitter;
use zbus::{connection, fdo, interface};

use crate::operations::task_ops::{
    AddExternalRefInput, AppendTaskNoteInput, CreateTaskInput, DeleteTaskInput, ListTasksInput,
    RelationshipInput, RepairTaskFrontmatterInput, SearchTasksInput, SetStatusInput, TaskLocator,
    UpdateTaskInput, add_deliverable, add_external_ref, append_task_note, create_task, delete_task,
    get_task, list_tasks, remove_deliverable, repair_task_frontmatter, search_tasks, set_status,
};
use crate::storage::Storage;

// ---- helpers ----------------------------------------------------------------

/// Map an internal error to a D-Bus `fdo::Error::Failed`.
fn map_err(e: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::Failed(e.to_string())
}

/// Serialize any serializable value to a JSON string for D-Bus return.
fn to_json<T: serde::Serialize>(v: &T) -> fdo::Result<String> {
    serde_json::to_string(v).map_err(map_err)
}

// ---- telemetry ---------------------------------------------------------------

/// Wrap one D-Bus operation with a span, a call counter and a latency
/// histogram, mirroring what `mcp-core` already records for `tools/call`.
///
/// `operation` is `&'static str` so the signature makes it impossible to pass
/// a caller-supplied string — a task title, a search query, a note — as the
/// span field or the metric label: only a fixed vocabulary of literals chosen
/// at each call site compiles. The wrapper never inspects `fut`'s output
/// beyond whether it is `Ok` or `Err`, so it cannot leak whatever the
/// operation itself returns or logs.
async fn record_dbus_call<T>(
    operation: &'static str,
    fut: impl Future<Output = fdo::Result<T>>,
) -> fdo::Result<T> {
    let span = tracing::info_span!("tasks_mcp.dbus.call", operation);
    async {
        let started = Instant::now();
        let result = fut.await;
        let outcome: &'static str = if result.is_ok() { "ok" } else { "error" };
        metrics::increment(
            "tasks_mcp.dbus.call",
            &[
                Label::new("operation", operation),
                Label::new("outcome", outcome),
            ],
        );
        metrics::record_duration(
            "tasks_mcp.dbus.call.duration",
            started.elapsed(),
            &[Label::new("operation", operation)],
        );
        result
    }
    .instrument(span)
    .await
}

// ---- interface struct -------------------------------------------------------

/// Holds the shared storage handle for the D-Bus interface implementation.
pub struct TasksInterface {
    storage: Storage,
}

impl TasksInterface {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }
}

// ---- zbus interface ---------------------------------------------------------

#[interface(name = "org.tasks.TasksMcp")]
impl TasksInterface {
    // ---- signals ------------------------------------------------------------

    /// Emitted after any operation that mutates task data.
    #[zbus(signal)]
    pub async fn tasks_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    // ---- read-only operations -----------------------------------------------

    /// Return a JSON array of all task list names.
    async fn list_lists(&self) -> fdo::Result<String> {
        record_dbus_call("list_lists", async {
            let lists = self.storage.list_lists().await.map_err(map_err)?;
            to_json(&lists)
        })
        .await
    }

    /// Return a JSON array of task summaries.  `input_json` is a
    /// `ListTasksInput` object serialised to JSON (all fields optional).
    async fn list_tasks(&self, input_json: &str) -> fdo::Result<String> {
        record_dbus_call("list_tasks", async {
            let input: ListTasksInput = serde_json::from_str(input_json).map_err(map_err)?;
            let result = list_tasks(&self.storage, input).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Return a JSON task document for the given id or file path.
    /// Pass an empty string for whichever locator you are not using.
    async fn get_task(&self, id: &str, path: &str) -> fdo::Result<String> {
        record_dbus_call("get_task", async {
            let locator = TaskLocator {
                id: non_empty(id),
                path: non_empty(path),
            };
            let result = get_task(&self.storage, locator).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Full-text search.  `input_json` is a `SearchTasksInput` object.
    async fn search_tasks(&self, input_json: &str) -> fdo::Result<String> {
        record_dbus_call("search_tasks", async {
            let input: SearchTasksInput = serde_json::from_str(input_json).map_err(map_err)?;
            let result = search_tasks(&self.storage, input).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    // ---- write operations (each emits TasksChanged) -------------------------

    /// Create a new task list directory.  Returns `{"created":true,"name":"…"}`.
    async fn create_list(
        &self,
        name: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("create_list", async {
            self.storage.create_list(name).await.map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&json!({"created": true, "name": name}))
        })
        .await
    }

    /// Create a task.  `input_json` is a `CreateTaskInput` object.
    /// Returns `{"id":"…","path":"…"}`.
    async fn create_task(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("create_task", async {
            let input: CreateTaskInput = serde_json::from_str(input_json).map_err(map_err)?;
            let result = create_task(&self.storage, input).await.map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Update a task's frontmatter / body.  `input_json` is an `UpdateTaskInput` object.
    async fn update_task(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("update_task", async {
            let input: UpdateTaskInput = serde_json::from_str(input_json).map_err(map_err)?;
            let result = crate::operations::task_ops::update_task(&self.storage, input)
                .await
                .map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Set a task's status.  `input_json` is a `SetStatusInput` object.
    async fn set_status(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("set_status", async {
            let input: SetStatusInput = serde_json::from_str(input_json).map_err(map_err)?;
            let result = set_status(&self.storage, input).await.map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Delete a task.  Pass an empty string for whichever locator you are not using.
    async fn delete_task(
        &self,
        id: &str,
        path: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("delete_task", async {
            let input = DeleteTaskInput {
                locator: TaskLocator {
                    id: non_empty(id),
                    path: non_empty(path),
                },
            };
            let result = delete_task(&self.storage, input).await.map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Link a deliverable to an epic.
    async fn add_deliverable(
        &self,
        epic_id: &str,
        deliverable_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("add_deliverable", async {
            let input = RelationshipInput {
                epic_id: epic_id.to_owned(),
                deliverable_id: deliverable_id.to_owned(),
            };
            let result = add_deliverable(&self.storage, input)
                .await
                .map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Unlink a deliverable from an epic.
    async fn remove_deliverable(
        &self,
        epic_id: &str,
        deliverable_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("remove_deliverable", async {
            let input = RelationshipInput {
                epic_id: epic_id.to_owned(),
                deliverable_id: deliverable_id.to_owned(),
            };
            let result = remove_deliverable(&self.storage, input)
                .await
                .map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Append a note to a task body.  `input_json` is an `AppendTaskNoteInput` object.
    async fn append_task_note(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("append_task_note", async {
            let input: AppendTaskNoteInput = serde_json::from_str(input_json).map_err(map_err)?;
            let result = append_task_note(&self.storage, input)
                .await
                .map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Add a structured external reference to a task.
    /// `input_json` is an `AddExternalRefInput` object.
    async fn add_external_ref(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("add_external_ref", async {
            let input: AddExternalRefInput = serde_json::from_str(input_json).map_err(map_err)?;
            let result = add_external_ref(&self.storage, input)
                .await
                .map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }

    /// Repair corrupt task frontmatter.
    /// `input_json` is a `RepairTaskFrontmatterInput` object.
    async fn repair_task_frontmatter(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        record_dbus_call("repair_task_frontmatter", async {
            let input: RepairTaskFrontmatterInput =
                serde_json::from_str(input_json).map_err(map_err)?;
            let result = repair_task_frontmatter(&self.storage, input)
                .await
                .map_err(map_err)?;
            Self::tasks_changed(&emitter).await.map_err(map_err)?;
            to_json(&result)
        })
        .await
    }
}

// ---- small helper -----------------------------------------------------------

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_owned())
    }
}

// ---- startup errors -----------------------------------------------------------

/// Why the D-Bus service failed to start.
///
/// Classified from the `zbus::Error` variant, never from its message text
/// (the shared Rust conventions forbid matching on an error's rendered
/// string). Bounded to three reasons so a metric label built from it never
/// grows without limit.
#[derive(Debug, Error)]
pub enum DbusStartupError {
    /// No session bus is reachable — the classic headless or container
    /// condition. Absent capability under the platform's degrade-cleanly
    /// rule (see `AGENTS.md`), not a fault.
    #[error("no session bus available: {0}")]
    NoSessionBus(#[source] zbus::Error),
    /// `org.tasks.TasksMcp` is already owned by another process. A genuine
    /// misconfiguration: two D-Bus services competing for one name.
    #[error("failed to claim bus name org.tasks.TasksMcp: {0}")]
    NameTaken(#[source] zbus::Error),
    /// Every other setup failure — a rejected handshake, a malformed reply,
    /// an object registration failure. A genuine failure.
    #[error("D-Bus setup failed: {0}")]
    SetupFailed(#[source] zbus::Error),
}

impl DbusStartupError {
    /// Classify a raw `zbus::Error` from the connection-build chain into the
    /// bounded reason vocabulary above.
    ///
    /// `zbus::Error` is `#[non_exhaustive]`, so the wildcard arm is required
    /// — and it is the right place for one: an upstream variant this crate
    /// has never heard of still falls into `SetupFailed`, the conservative
    /// "genuine failure" bucket, rather than failing to compile.
    fn classify(err: zbus::Error) -> Self {
        match &err {
            zbus::Error::InputOutput(_) => Self::NoSessionBus(err),
            zbus::Error::NameTaken => Self::NameTaken(err),
            _ => Self::SetupFailed(err),
        }
    }

    /// The bounded `reason` label for the `tasks_mcp.dbus.startup_failure`
    /// counter.
    ///
    /// Deliberately exhaustive with no wildcard arm: adding a fourth variant
    /// to this enum without extending this match is a compile error, so a
    /// new reason can never land uncounted.
    pub fn metric_reason(&self) -> &'static str {
        match self {
            Self::NoSessionBus(_) => "no_session_bus",
            Self::NameTaken(_) => "name_taken",
            Self::SetupFailed(_) => "setup_failed",
        }
    }
}

impl From<DbusStartupError> for crate::error::TaskMcpError {
    fn from(err: DbusStartupError) -> Self {
        crate::error::TaskMcpError::Internal(err.to_string())
    }
}

// ---- service runner ---------------------------------------------------------

/// Register the `org.tasks.TasksMcp` service on the session bus and serve
/// requests until the process exits.
///
/// Designed to run as a long-lived tokio task alongside other transports, or
/// as the sole responsibility of `tasks-mcp dbus`.
///
/// On failure, this records the `tasks_mcp.dbus.startup_failure` counter
/// (labelled by [`DbusStartupError::metric_reason`]) before returning, so a
/// missing session bus or a lost name claim is visible even without a
/// collector attached. The caller decides how loudly to log it — main.rs
/// applies rule 8.2 there — because only the caller knows whether this is the
/// `serve` command's best-effort side channel or the `dbus` subcommand's
/// entire job.
pub async fn run_dbus_service(storage: Storage) -> Result<(), DbusStartupError> {
    let interface = TasksInterface::new(storage);

    let build = async {
        let conn = connection::Builder::session()?
            .name("org.tasks.TasksMcp")?
            .serve_at("/org/tasks/TasksMcp", interface)?
            .build()
            .await?;
        Ok::<_, zbus::Error>(conn)
    };

    let _conn = match build.await {
        Ok(conn) => conn,
        Err(err) => {
            let classified = DbusStartupError::classify(err);
            metrics::increment(
                "tasks_mcp.dbus.startup_failure",
                &[Label::new("reason", classified.metric_reason())],
            );
            return Err(classified);
        }
    };

    // The connection object must be kept alive for the service to remain
    // registered on the bus.  Park the task here until shutdown.
    std::future::pending::<()>().await;

    Ok(())
}

// ---- tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io;
    use std::sync::{Arc, Mutex};

    use serde_json::Value;
    use tempfile::TempDir;
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::registry::LookupSpan;

    use super::*;

    // ---- a small capturing layer, local to this module ---------------------
    //
    // Each test module that needs to read spans/events back gets its own copy
    // (mcp-core's own test suite does the same) rather than a shared crate,
    // because the shape each caller wants (which fields, which levels) is
    // small and differs.

    #[derive(Clone, Debug)]
    struct RecordedSpan {
        name: &'static str,
        fields: BTreeMap<String, String>,
    }

    #[derive(Clone, Debug)]
    struct RecordedEvent {
        level: tracing::Level,
        fields: BTreeMap<String, String>,
    }

    #[derive(Clone, Default)]
    struct Capture(
        Arc<Mutex<Vec<RecordedSpan>>>,
        Arc<Mutex<Vec<RecordedEvent>>>,
    );

    impl<S> Layer<S> for Capture
    where
        S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {
            let mut fields = BTreeMap::new();
            attrs.record(&mut Collector(&mut fields));
            self.0.lock().expect("capture lock").push(RecordedSpan {
                name: attrs.metadata().name(),
                fields,
            });
        }

        fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
            let name = ctx.span(id).map_or("<closed>", |span| span.name());
            let mut fields = BTreeMap::new();
            values.record(&mut Collector(&mut fields));
            self.0
                .lock()
                .expect("capture lock")
                .push(RecordedSpan { name, fields });
        }

        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut fields = BTreeMap::new();
            event.record(&mut Collector(&mut fields));
            self.1.lock().expect("capture lock").push(RecordedEvent {
                level: *event.metadata().level(),
                fields,
            });
        }
    }

    struct Collector<'a>(&'a mut BTreeMap<String, String>);

    impl Visit for Collector<'_> {
        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.insert(field.name().to_string(), value.to_string());
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0
                .insert(field.name().to_string(), format!("{value:?}"));
        }
    }

    /// Run `body` under a capturing subscriber and return what it recorded,
    /// alongside whatever `body` itself produced.
    fn capture<F, Fut, T>(body: F) -> (Vec<RecordedSpan>, Vec<RecordedEvent>, T)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        let value = tracing::subscriber::with_default(subscriber, || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("current-thread runtime");
            runtime.block_on(body())
        });
        let spans = capture.0.lock().expect("capture lock").clone();
        let events = capture.1.lock().expect("capture lock").clone();
        (spans, events, value)
    }

    fn any_field_contains(spans: &[RecordedSpan], needle: &str) -> bool {
        spans
            .iter()
            .any(|s| s.fields.values().any(|v| v.contains(needle)))
    }

    fn any_event_at_or_above_contains(
        events: &[RecordedEvent],
        max_level: tracing::Level,
        needle: &str,
    ) -> bool {
        events
            .iter()
            .any(|e| e.level <= max_level && e.fields.values().any(|v| v.contains(needle)))
    }

    // ---- DbusStartupError::classify -----------------------------------------

    fn io_error() -> zbus::Error {
        zbus::Error::InputOutput(Arc::new(io::Error::new(
            io::ErrorKind::NotFound,
            "no such file or directory (test)",
        )))
    }

    #[test]
    fn classify_maps_input_output_to_no_session_bus() {
        let classified = DbusStartupError::classify(io_error());
        assert!(
            matches!(classified, DbusStartupError::NoSessionBus(_)),
            "an InputOutput error must classify as NoSessionBus, got {classified:?}"
        );
    }

    #[test]
    fn classify_maps_name_taken_to_name_taken() {
        let classified = DbusStartupError::classify(zbus::Error::NameTaken);
        assert!(
            matches!(classified, DbusStartupError::NameTaken(_)),
            "a NameTaken error must classify as NameTaken, got {classified:?}"
        );
    }

    #[test]
    fn classify_maps_other_errors_to_setup_failed() {
        let classified = DbusStartupError::classify(zbus::Error::Handshake(
            "test: handshake rejected".to_string(),
        ));
        assert!(
            matches!(classified, DbusStartupError::SetupFailed(_)),
            "an unrecognised error must classify as SetupFailed, got {classified:?}"
        );
    }

    #[test]
    fn metric_reason_is_bounded_and_distinct_per_variant() {
        let reasons = [
            DbusStartupError::classify(io_error()).metric_reason(),
            DbusStartupError::classify(zbus::Error::NameTaken).metric_reason(),
            DbusStartupError::classify(zbus::Error::Handshake("x".to_string())).metric_reason(),
        ];
        let unique: std::collections::BTreeSet<_> = reasons.iter().collect();
        assert_eq!(
            unique.len(),
            3,
            "each startup-error variant must report its own bounded reason, got {reasons:?}"
        );
        for reason in reasons {
            assert!(
                ["no_session_bus", "name_taken", "setup_failed"].contains(&reason),
                "reason must come from the fixed vocabulary, got {reason}"
            );
        }
    }

    // ---- record_dbus_call ----------------------------------------------------

    /// A metric name recorded from more than one `#[test]` fn in this file
    /// would share a series in the process-global registry (adelie-telemetry
    /// accumulates in one static for the life of the process — see
    /// `adelie-ai/adelie-telemetry#6`). This mutex serialises every test that
    /// touches it so a before/after snapshot delta is never read mid-write by
    /// a concurrently running test in this same file.
    static METRICS_LOCK: Mutex<()> = Mutex::new(());

    fn counter_total(
        summary: &metrics::Summary,
        name: &str,
        operation: &str,
        outcome: &str,
    ) -> u64 {
        summary
            .counters
            .iter()
            .filter(|c| c.name == name)
            .filter(|c| {
                c.labels
                    .iter()
                    .any(|l| l.key() == "operation" && l.value() == operation)
            })
            .filter(|c| {
                c.labels
                    .iter()
                    .any(|l| l.key() == "outcome" && l.value() == outcome)
            })
            .map(|c| c.total)
            .sum()
    }

    fn histogram_count(summary: &metrics::Summary, name: &str, operation: &str) -> u64 {
        summary
            .histograms
            .iter()
            .filter(|h| h.name == name)
            .filter(|h| {
                h.labels
                    .iter()
                    .any(|l| l.key() == "operation" && l.value() == operation)
            })
            .map(|h| h.total.count)
            .sum()
    }

    #[test]
    fn record_dbus_call_span_carries_only_the_operation_field() {
        let (spans, _events, ()) = capture(|| async {
            let _: fdo::Result<&str> =
                record_dbus_call("probe_operation", async { Ok("value") }).await;
        });

        let span = spans
            .iter()
            .find(|s| s.name == "tasks_mcp.dbus.call")
            .expect("record_dbus_call must open a tasks_mcp.dbus.call span");
        assert_eq!(
            span.fields.get("operation").map(String::as_str),
            Some("probe_operation"),
            "the span must carry the operation name, got {:?}",
            span.fields
        );
        assert_eq!(
            span.fields.len(),
            1,
            "the span must carry only the operation field, got {:?}",
            span.fields
        );
    }

    #[test]
    fn record_dbus_call_records_counter_and_duration() {
        let _guard = METRICS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let before = metrics::global().snapshot();

        capture(|| async {
            let _: fdo::Result<&str> =
                record_dbus_call("metrics_probe_ok", async { Ok("value") }).await;
            let _: fdo::Result<&str> = record_dbus_call("metrics_probe_err", async {
                Err(fdo::Error::Failed("boom".to_string()))
            })
            .await;
        });

        let after = metrics::global().snapshot();

        let ok_before = counter_total(&before, "tasks_mcp.dbus.call", "metrics_probe_ok", "ok");
        let ok_after = counter_total(&after, "tasks_mcp.dbus.call", "metrics_probe_ok", "ok");
        assert_eq!(
            ok_after,
            ok_before + 1,
            "a successful call must add one to the ok counter"
        );

        let err_before =
            counter_total(&before, "tasks_mcp.dbus.call", "metrics_probe_err", "error");
        let err_after = counter_total(&after, "tasks_mcp.dbus.call", "metrics_probe_err", "error");
        assert_eq!(
            err_after,
            err_before + 1,
            "a failed call must add one to the error counter"
        );

        let hist_before =
            histogram_count(&before, "tasks_mcp.dbus.call.duration", "metrics_probe_ok");
        let hist_after =
            histogram_count(&after, "tasks_mcp.dbus.call.duration", "metrics_probe_ok");
        assert_eq!(
            hist_after,
            hist_before + 1,
            "a call must add one measurement to the duration histogram"
        );
    }

    // ---- the D-Bus surface does not leak task content -------------------------

    fn test_storage() -> (TempDir, Storage) {
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = Storage::with_root(dir.path());
        (dir, storage)
    }

    /// A synthetic, unauthenticated peer-to-peer connection: enough for
    /// `SignalEmitter::new` to hand back a real emitter without a session
    /// bus. `_peer` is the other half of the pair; it must stay alive for the
    /// life of the connection or writes to `conn` fail with a broken pipe.
    async fn test_connection() -> (zbus::Connection, tokio::net::UnixStream) {
        let (a, b) = tokio::net::UnixStream::pair().expect("unix socket pair");
        let guid = zbus::Guid::from_static_str("0123456789abcdef0123456789abcdef")
            .expect("well-formed test GUID");
        let conn = connection::Builder::authenticated_socket(a, guid)
            .expect("authenticated_socket builder")
            .build()
            .await
            .expect("build a synthetic connection for testing signal emission");
        (conn, b)
    }

    /// A distinguishable, per-operation sentinel, so a leak names which
    /// operation produced it — the same reasoning as
    /// `tests/support/mod.rs::tag` on the MCP side.
    fn tag(operation: &str) -> String {
        format!("sentinel-dbus-leak-check-8f3a1c2e-{operation}")
    }

    /// Every operation `TasksInterface` exposes on the D-Bus surface.
    ///
    /// This is a hand-maintained manifest, not derived from a live,
    /// introspectable source the way the MCP test derives its list from
    /// `tool_definitions()` — zbus's `#[interface]` macro does not expose the
    /// method list to a unit test. Adding a fifteenth D-Bus method without
    /// adding it here and to `run_dbus_content_leak_flow` below will not fail
    /// to compile or fail this test; it relies on the diff being visible in
    /// review. Keep the two in sync.
    const DBUS_OPERATIONS: &[&str] = &[
        "list_lists",
        "list_tasks",
        "get_task",
        "search_tasks",
        "create_list",
        "create_task",
        "update_task",
        "set_status",
        "delete_task",
        "add_deliverable",
        "remove_deliverable",
        "append_task_note",
        "add_external_ref",
        "repair_task_frontmatter",
    ];

    /// Call every operation in [`DBUS_OPERATIONS`] on `interface`, in an
    /// order that satisfies each one's own data dependencies, planting a
    /// distinct sentinel in each call that takes a content-bearing argument.
    /// Returns every sentinel planted, so the caller can assert none of them
    /// reached a span field or an event.
    async fn run_dbus_content_leak_flow(interface: &TasksInterface) -> Vec<String> {
        let (conn, _peer) = test_connection().await;
        let emitter = SignalEmitter::new(&conn, "/org/tasks/TasksMcp")
            .expect("construct a signal emitter over the synthetic connection");
        let mut sentinels = Vec::new();

        let list_name = tag("create_list");
        sentinels.push(list_name.clone());
        let _ = interface.create_list(&list_name, emitter.clone()).await;
        let _ = interface.list_lists().await;

        let epic_title = tag("create_task-epic");
        sentinels.push(epic_title.clone());
        let epic_input =
            json!({"list": list_name, "type": "epic", "title": epic_title}).to_string();
        let epic_resp = interface
            .create_task(&epic_input, emitter.clone())
            .await
            .expect("create epic");
        let epic_id: Value = serde_json::from_str(&epic_resp).expect("epic response json");
        let epic_id = epic_id["id"].as_str().expect("epic id").to_string();

        let deliverable_title = tag("create_task-deliverable");
        sentinels.push(deliverable_title.clone());
        let deliverable_input =
            json!({"list": list_name, "type": "deliverable", "title": deliverable_title})
                .to_string();
        let deliverable_resp = interface
            .create_task(&deliverable_input, emitter.clone())
            .await
            .expect("create deliverable");
        let deliverable_id: Value =
            serde_json::from_str(&deliverable_resp).expect("deliverable response json");
        let deliverable_id = deliverable_id["id"]
            .as_str()
            .expect("deliverable id")
            .to_string();

        let disposable_title = tag("delete_task-target");
        sentinels.push(disposable_title.clone());
        let disposable_input =
            json!({"list": list_name, "type": "deliverable", "title": disposable_title})
                .to_string();
        let disposable_resp = interface
            .create_task(&disposable_input, emitter.clone())
            .await
            .expect("create disposable task");
        let disposable_id: Value =
            serde_json::from_str(&disposable_resp).expect("disposable response json");
        let disposable_id = disposable_id["id"]
            .as_str()
            .expect("disposable id")
            .to_string();

        let _ = interface.get_task(&deliverable_id, "").await;

        let update_note = tag("update_task");
        sentinels.push(update_note.clone());
        let update_input =
            json!({"id": deliverable_id, "patch": {"body_append": update_note}}).to_string();
        let _ = interface.update_task(&update_input, emitter.clone()).await;

        let set_status_input = json!({"id": deliverable_id, "status": "doing"}).to_string();
        let _ = interface
            .set_status(&set_status_input, emitter.clone())
            .await;

        let list_tasks_input = json!({"lists": [list_name]}).to_string();
        let _ = interface.list_tasks(&list_tasks_input).await;

        let search_text = tag("search_tasks");
        sentinels.push(search_text.clone());
        let search_input = json!({"text": search_text}).to_string();
        let _ = interface.search_tasks(&search_input).await;

        let _ = interface
            .add_deliverable(&epic_id, &deliverable_id, emitter.clone())
            .await;
        let _ = interface
            .remove_deliverable(&epic_id, &deliverable_id, emitter.clone())
            .await;

        let note = tag("append_task_note");
        sentinels.push(note.clone());
        let note_input = json!({"id": deliverable_id, "note": note}).to_string();
        let _ = interface
            .append_task_note(&note_input, emitter.clone())
            .await;

        let external_ref = tag("add_external_ref");
        sentinels.push(external_ref.clone());
        let ref_input =
            json!({"id": deliverable_id, "system": "github", "ref": external_ref}).to_string();
        let _ = interface
            .add_external_ref(&ref_input, emitter.clone())
            .await;

        let repair_input =
            json!({"id": deliverable_id, "strategy": "salvage", "dry_run": true}).to_string();
        let _ = interface
            .repair_task_frontmatter(&repair_input, emitter.clone())
            .await;

        let _ = interface
            .delete_task(&disposable_id, "", emitter.clone())
            .await;

        // ---- failure paths (lesson 9) ---------------------------------------
        //
        // Driving only the success branch above never runs this crate's own
        // error-message construction, and that is exactly where a sibling
        // server's leak was found: an error's `Display` quoting back the
        // argument that failed. `get_task` with a path outside the tasks
        // root is the known case (`src/operations/task_ops.rs`'s
        // `format!("path '{}' is outside the tasks root ...")`); the
        // `NotFound` calls exercise every other write operation's own error
        // branch.

        let missing_id = tag("not-found-id");
        sentinels.push(missing_id.clone());
        assert!(
            interface.get_task(&missing_id, "").await.is_err(),
            "get_task with a missing id must fail so its error branch runs"
        );
        let update_missing = json!({"id": missing_id, "patch": {}}).to_string();
        assert!(
            interface
                .update_task(&update_missing, emitter.clone())
                .await
                .is_err(),
            "update_task with a missing id must fail so its error branch runs"
        );
        let note_missing = json!({"id": missing_id, "note": "x"}).to_string();
        assert!(
            interface
                .append_task_note(&note_missing, emitter.clone())
                .await
                .is_err(),
            "append_task_note with a missing id must fail so its error branch runs"
        );
        assert!(
            interface
                .add_deliverable(&missing_id, &deliverable_id, emitter.clone())
                .await
                .is_err(),
            "add_deliverable with a missing epic id must fail so its error branch runs"
        );

        let outside_root_sentinel = tag("outside-root-path");
        sentinels.push(outside_root_sentinel.clone());
        let outside_path = format!("/tmp/{outside_root_sentinel}-outside-root.md");
        assert!(
            interface.get_task("", &outside_path).await.is_err(),
            "get_task with a path outside the tasks root must fail so its error branch runs"
        );

        let invalid_list_sentinel = tag("create_list-invalid");
        sentinels.push(invalid_list_sentinel.clone());
        let bad_list_name = format!("{invalid_list_sentinel}/../invalid");
        assert!(
            interface
                .create_list(&bad_list_name, emitter)
                .await
                .is_err(),
            "create_list with an invalid name must fail so its error branch runs"
        );

        sentinels
    }

    /// Acceptance: every operation `DBUS_OPERATIONS` names can be called end
    /// to end with a sentinel in its content-bearing argument, and none of
    /// them ever reaches a `tasks_mcp.dbus.call` span field or any event
    /// (lesson 8: table-driven over the whole operation list, not one
    /// hand-picked operation). `record_dbus_call`'s own leak-safety is tested
    /// generically above; this test guards the 14 call sites that use it.
    #[test]
    fn every_dbus_operation_is_exercised_without_leaking_content() {
        let (_dir, storage) = test_storage();
        let interface = TasksInterface::new(storage);

        let (spans, events, sentinels) =
            capture(|| async { run_dbus_content_leak_flow(&interface).await });

        let exercised: std::collections::BTreeSet<&str> = spans
            .iter()
            .filter(|s| s.name == "tasks_mcp.dbus.call")
            .filter_map(|s| s.fields.get("operation").map(String::as_str))
            .collect();
        let expected: std::collections::BTreeSet<&str> = DBUS_OPERATIONS.iter().copied().collect();
        assert_eq!(
            exercised,
            expected,
            "every operation in DBUS_OPERATIONS must produce a tasks_mcp.dbus.call span; \
             missing: {:?}, unexpected: {:?}",
            expected.difference(&exercised).collect::<Vec<_>>(),
            exercised.difference(&expected).collect::<Vec<_>>()
        );

        for sentinel in &sentinels {
            assert!(
                !any_field_contains(&spans, sentinel),
                "a span field leaked content for sentinel {sentinel}"
            );
            assert!(
                !any_event_at_or_above_contains(&events, tracing::Level::TRACE, sentinel),
                "an event leaked content for sentinel {sentinel} (the D-Bus surface logs no \
                 argument content at any level, so none should ever appear)"
            );
        }
    }
}
