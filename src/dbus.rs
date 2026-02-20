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

#![deny(warnings)]

use serde_json::json;
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
        let lists = self.storage.list_lists().await.map_err(map_err)?;
        to_json(&lists)
    }

    /// Return a JSON array of task summaries.  `input_json` is a
    /// `ListTasksInput` object serialised to JSON (all fields optional).
    async fn list_tasks(&self, input_json: &str) -> fdo::Result<String> {
        let input: ListTasksInput =
            serde_json::from_str(input_json).map_err(map_err)?;
        let result = list_tasks(&self.storage, input).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Return a JSON task document for the given id or file path.
    /// Pass an empty string for whichever locator you are not using.
    async fn get_task(&self, id: &str, path: &str) -> fdo::Result<String> {
        let locator = TaskLocator {
            id: non_empty(id),
            path: non_empty(path),
        };
        let result = get_task(&self.storage, locator).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Full-text search.  `input_json` is a `SearchTasksInput` object.
    async fn search_tasks(&self, input_json: &str) -> fdo::Result<String> {
        let input: SearchTasksInput =
            serde_json::from_str(input_json).map_err(map_err)?;
        let result = search_tasks(&self.storage, input).await.map_err(map_err)?;
        to_json(&result)
    }

    // ---- write operations (each emits TasksChanged) -------------------------

    /// Create a new task list directory.  Returns `{"created":true,"name":"…"}`.
    async fn create_list(
        &self,
        name: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        self.storage.create_list(name).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&json!({"created": true, "name": name}))
    }

    /// Create a task.  `input_json` is a `CreateTaskInput` object.
    /// Returns `{"id":"…","path":"…"}`.
    async fn create_task(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input: CreateTaskInput =
            serde_json::from_str(input_json).map_err(map_err)?;
        let result = create_task(&self.storage, input).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Update a task's frontmatter / body.  `input_json` is an `UpdateTaskInput` object.
    async fn update_task(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input: UpdateTaskInput =
            serde_json::from_str(input_json).map_err(map_err)?;
        let result =
            crate::operations::task_ops::update_task(&self.storage, input)
                .await
                .map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Set a task's status.  `input_json` is a `SetStatusInput` object.
    async fn set_status(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input: SetStatusInput =
            serde_json::from_str(input_json).map_err(map_err)?;
        let result = set_status(&self.storage, input).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Delete a task.  Pass an empty string for whichever locator you are not using.
    async fn delete_task(
        &self,
        id: &str,
        path: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input = DeleteTaskInput {
            locator: TaskLocator {
                id: non_empty(id),
                path: non_empty(path),
            },
        };
        let result = delete_task(&self.storage, input).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Link a deliverable to an epic.
    async fn add_deliverable(
        &self,
        epic_id: &str,
        deliverable_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input = RelationshipInput {
            epic_id: epic_id.to_owned(),
            deliverable_id: deliverable_id.to_owned(),
        };
        let result = add_deliverable(&self.storage, input).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Unlink a deliverable from an epic.
    async fn remove_deliverable(
        &self,
        epic_id: &str,
        deliverable_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input = RelationshipInput {
            epic_id: epic_id.to_owned(),
            deliverable_id: deliverable_id.to_owned(),
        };
        let result =
            remove_deliverable(&self.storage, input).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Append a note to a task body.  `input_json` is an `AppendTaskNoteInput` object.
    async fn append_task_note(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input: AppendTaskNoteInput =
            serde_json::from_str(input_json).map_err(map_err)?;
        let result = append_task_note(&self.storage, input).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Add a structured external reference to a task.
    /// `input_json` is an `AddExternalRefInput` object.
    async fn add_external_ref(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input: AddExternalRefInput =
            serde_json::from_str(input_json).map_err(map_err)?;
        let result = add_external_ref(&self.storage, input).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }

    /// Repair corrupt task frontmatter.
    /// `input_json` is a `RepairTaskFrontmatterInput` object.
    async fn repair_task_frontmatter(
        &self,
        input_json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let input: RepairTaskFrontmatterInput =
            serde_json::from_str(input_json).map_err(map_err)?;
        let result =
            repair_task_frontmatter(&self.storage, input).await.map_err(map_err)?;
        Self::tasks_changed(&emitter).await.map_err(map_err)?;
        to_json(&result)
    }
}

// ---- small helper -----------------------------------------------------------

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() { None } else { Some(s.to_owned()) }
}

// ---- service runner ---------------------------------------------------------

/// Register the `org.tasks.TasksMcp` service on the session bus and serve
/// requests until the process exits.
///
/// Designed to run as a long-lived tokio task alongside other transports, or
/// as the sole responsibility of `tasks-mcp dbus`.
pub async fn run_dbus_service(storage: Storage) -> crate::error::Result<()> {
    let interface = TasksInterface::new(storage);

    let _conn = connection::Builder::session()
        .map_err(|e| crate::error::TaskMcpError::Internal(e.to_string()))?
        .name("org.tasks.TasksMcp")
        .map_err(|e| crate::error::TaskMcpError::Internal(e.to_string()))?
        .serve_at("/org/tasks/TasksMcp", interface)
        .map_err(|e| crate::error::TaskMcpError::Internal(e.to_string()))?
        .build()
        .await
        .map_err(|e| crate::error::TaskMcpError::Internal(e.to_string()))?;

    // The connection object must be kept alive for the service to remain
    // registered on the bus.  Park the task here until shutdown.
    std::future::pending::<()>().await;

    Ok(())
}
