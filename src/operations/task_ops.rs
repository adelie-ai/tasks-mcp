#![deny(warnings)]

use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::{Value, json};
use tokio::fs;

use crate::error::{Result, TaskMcpError};
use crate::markdown::{parse_task_markdown, render_task_markdown, validate_frontmatter};
use crate::model::{ExternalRef, TaskDocument, TaskFrontmatter, TaskStatus, TaskSummary, TaskType};
use crate::storage::{Storage, generate_task_id, now_iso8601};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateTaskInput {
    pub list: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub title: String,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub epic_id: Option<String>,
    #[serde(default)]
    pub deliverable_ids: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub priority: Option<crate::model::Priority>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub links: Option<Vec<String>>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskLocator {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct UpdateTaskInput {
    #[serde(flatten)]
    pub locator: TaskLocator,
    #[serde(default)]
    pub patch: Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeleteTaskInput {
    #[serde(flatten)]
    pub locator: TaskLocator,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SetStatusInput {
    #[serde(flatten)]
    pub locator: TaskLocator,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ListTasksInput {
    #[serde(default)]
    pub list: Option<String>,
    #[serde(default)]
    pub lists: Option<Vec<String>>,
    #[serde(default, rename = "type")]
    pub task_type: Option<TaskType>,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub epic_id: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchTasksInput {
    pub text: String,
    #[serde(default)]
    pub lists: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RelationshipInput {
    pub epic_id: String,
    pub deliverable_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AppendTaskNoteInput {
    #[serde(flatten)]
    pub locator: TaskLocator,
    pub note: String,
    /// Optional heading name to insert under (e.g. "Notes" → `## Notes`). If the heading does
    /// not exist it is created. If `None`, the note is appended at the end of the body.
    #[serde(default)]
    pub section: Option<String>,
    /// Prefix the note with an ISO-8601 date. Defaults to `true`.
    #[serde(default)]
    pub timestamp: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AddExternalRefInput {
    #[serde(flatten)]
    pub locator: TaskLocator,
    pub system: String,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RepairStrategy {
    /// Try to keep body intact; move the corrupt YAML block under `## Recovered Frontmatter`.
    Salvage,
    /// Reconstruct frontmatter entirely from file-path metadata; preserve raw body.
    Reset,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RepairTaskFrontmatterInput {
    #[serde(flatten)]
    pub locator: TaskLocator,
    pub strategy: RepairStrategy,
    /// When `true`, return the repaired content without writing to disk.
    #[serde(default)]
    pub dry_run: Option<bool>,
}

pub async fn create_task(storage: &Storage, input: CreateTaskInput) -> Result<Value> {
    storage.create_list(&input.list).await?;

    let id = generate_task_id();
    let now = now_iso8601();
    let mut frontmatter = TaskFrontmatter {
        id: id.clone(),
        title: input.title.clone(),
        task_type: input.task_type,
        status: input.status.unwrap_or(TaskStatus::Todo),
        list: input.list.clone(),
        created: now.clone(),
        updated: now,
        epic_id: input.epic_id,
        deliverable_ids: input.deliverable_ids,
        tags: input.tags,
        priority: input.priority,
        due: input.due,
        links: input.links,
        assignee: input.assignee,
        external_refs: None,
    };

    if frontmatter.task_type == TaskType::Epic {
        frontmatter.epic_id = None;
    }
    if frontmatter.task_type == TaskType::Deliverable {
        frontmatter.deliverable_ids = None;
    }

    validate_frontmatter(&frontmatter)?;

    let body = input.body.unwrap_or_default();
    let path = storage.task_file_path(
        &frontmatter.list,
        frontmatter.task_type,
        &id,
        &frontmatter.title,
    );
    let markdown = render_task_markdown(&frontmatter, &body)?;
    storage.atomic_write(&path, &markdown).await?;

    Ok(json!({"id": id, "path": path.to_string_lossy()}))
}

pub async fn get_task(storage: &Storage, locator: TaskLocator) -> Result<Value> {
    let path = locate_task_path(storage, &locator).await?;
    let document = read_task_from_path(&path).await?;
    Ok(serde_json::to_value(document)?)
}

pub async fn update_task(storage: &Storage, input: UpdateTaskInput) -> Result<Value> {
    let path = locate_task_path(storage, &input.locator).await?;
    let mut document = read_task_from_path(&path).await?;

    apply_patch(&mut document, &input.patch)?;
    document.frontmatter.updated = now_iso8601();
    validate_frontmatter(&document.frontmatter)?;

    let new_path = storage.task_file_path(
        &document.frontmatter.list,
        document.frontmatter.task_type,
        &document.frontmatter.id,
        &document.frontmatter.title,
    );

    let markdown = render_task_markdown(&document.frontmatter, &document.body)?;
    storage.atomic_write(&new_path, &markdown).await?;

    if new_path != path && path.exists() {
        fs::remove_file(path).await?;
    }

    Ok(json!({"id": document.frontmatter.id, "path": new_path.to_string_lossy()}))
}

pub async fn delete_task(storage: &Storage, input: DeleteTaskInput) -> Result<Value> {
    let path = locate_task_path(storage, &input.locator).await?;
    fs::remove_file(&path).await?;
    Ok(json!({"deleted": true, "path": path.to_string_lossy()}))
}

pub async fn set_status(storage: &Storage, input: SetStatusInput) -> Result<Value> {
    let path = locate_task_path(storage, &input.locator).await?;
    let mut document = read_task_from_path(&path).await?;

    document.frontmatter.status = input.status;
    document.frontmatter.updated = now_iso8601();
    validate_frontmatter(&document.frontmatter)?;

    let new_path = storage.task_file_path(
        &document.frontmatter.list,
        document.frontmatter.task_type,
        &document.frontmatter.id,
        &document.frontmatter.title,
    );

    let markdown = render_task_markdown(&document.frontmatter, &document.body)?;
    storage.atomic_write(&new_path, &markdown).await?;

    if new_path != path && path.exists() {
        fs::remove_file(path).await?;
    }

    Ok(json!({
        "id": document.frontmatter.id,
        "path": new_path.to_string_lossy(),
        "status": input.status,
    }))
}

pub async fn list_tasks(storage: &Storage, input: ListTasksInput) -> Result<Value> {
    let mut paths = Vec::new();
    if let Some(lists) = &input.lists {
        for list_name in lists {
            let mut list_paths = storage
                .all_task_paths(Some(list_name), input.task_type)
                .await?;
            paths.append(&mut list_paths);
        }
    } else if let Some(list_name) = &input.list {
        paths = storage
            .all_task_paths(Some(list_name), input.task_type)
            .await?;
    } else {
        paths = storage.all_task_paths(None, input.task_type).await?;
    }

    paths.sort();
    paths.dedup();
    let mut summaries = Vec::new();

    for path in paths {
        let task = read_task_from_path(&path).await?;
        if let Some(status) = input.status
            && task.frontmatter.status != status
        {
            continue;
        }
        if let Some(tag) = &input.tag {
            let tags = task.frontmatter.tags.clone().unwrap_or_default();
            if !tags.iter().any(|t| t == tag) {
                continue;
            }
        }
        if let Some(assignee) = &input.assignee
            && task.frontmatter.assignee.as_deref() != Some(assignee.as_str())
        {
            continue;
        }
        if let Some(epic_id) = &input.epic_id
            && task.frontmatter.epic_id.as_deref() != Some(epic_id.as_str())
        {
            continue;
        }

        summaries.push(TaskSummary::from(&task));
    }

    Ok(serde_json::to_value(summaries)?)
}

pub async fn search_tasks(storage: &Storage, input: SearchTasksInput) -> Result<Value> {
    if input.text.trim().is_empty() {
        return Err(TaskMcpError::InvalidArgument(
            "search text cannot be empty".to_string(),
        ));
    }

    let needle = input.text.to_lowercase();
    let mut paths = Vec::new();
    if let Some(lists) = &input.lists {
        for list_name in lists {
            let mut list_paths = storage.all_task_paths(Some(list_name), None).await?;
            paths.append(&mut list_paths);
        }
    } else {
        paths = storage.all_task_paths(None, None).await?;
    }

    paths.sort();
    paths.dedup();

    let mut matches = Vec::new();
    for path in paths {
        let task = read_task_from_path(&path).await?;
        let title = task.frontmatter.title.to_lowercase();
        let body = task.body.to_lowercase();
        if title.contains(&needle) || body.contains(&needle) {
            matches.push(TaskSummary::from(&task));
        }
    }

    Ok(serde_json::to_value(matches)?)
}

pub async fn add_deliverable(storage: &Storage, input: RelationshipInput) -> Result<Value> {
    let epic_path = storage.find_task_path_by_id(&input.epic_id).await?;
    let deliverable_path = storage.find_task_path_by_id(&input.deliverable_id).await?;

    let mut epic = read_task_from_path(&epic_path).await?;
    let mut deliverable = read_task_from_path(&deliverable_path).await?;

    if epic.frontmatter.task_type != TaskType::Epic {
        return Err(TaskMcpError::InvalidArgument(
            "epic_id must reference a task of type epic".to_string(),
        ));
    }
    if deliverable.frontmatter.task_type != TaskType::Deliverable {
        return Err(TaskMcpError::InvalidArgument(
            "deliverable_id must reference a task of type deliverable".to_string(),
        ));
    }

    if let Some(existing) = deliverable.frontmatter.epic_id.as_deref()
        && existing != epic.frontmatter.id
    {
        return Err(TaskMcpError::Conflict(format!(
            "deliverable already assigned to epic {existing}"
        )));
    }

    deliverable.frontmatter.epic_id = Some(epic.frontmatter.id.clone());
    let mut deliverable_ids = epic.frontmatter.deliverable_ids.unwrap_or_default();
    if !deliverable_ids
        .iter()
        .any(|id| id == &deliverable.frontmatter.id)
    {
        deliverable_ids.push(deliverable.frontmatter.id.clone());
    }
    deliverable_ids.sort();
    deliverable_ids.dedup();
    epic.frontmatter.deliverable_ids = Some(deliverable_ids);

    epic.frontmatter.updated = now_iso8601();
    deliverable.frontmatter.updated = now_iso8601();

    persist_task(storage, &epic).await?;
    persist_task(storage, &deliverable).await?;

    Ok(json!({"ok": true}))
}

pub async fn remove_deliverable(storage: &Storage, input: RelationshipInput) -> Result<Value> {
    let epic_path = storage.find_task_path_by_id(&input.epic_id).await?;
    let deliverable_path = storage.find_task_path_by_id(&input.deliverable_id).await?;

    let mut epic = read_task_from_path(&epic_path).await?;
    let mut deliverable = read_task_from_path(&deliverable_path).await?;

    if epic.frontmatter.task_type != TaskType::Epic {
        return Err(TaskMcpError::InvalidArgument(
            "epic_id must reference a task of type epic".to_string(),
        ));
    }
    if deliverable.frontmatter.task_type != TaskType::Deliverable {
        return Err(TaskMcpError::InvalidArgument(
            "deliverable_id must reference a task of type deliverable".to_string(),
        ));
    }

    if let Some(ids) = epic.frontmatter.deliverable_ids.as_mut() {
        ids.retain(|id| id != &deliverable.frontmatter.id);
        if ids.is_empty() {
            epic.frontmatter.deliverable_ids = None;
        }
    }
    if deliverable.frontmatter.epic_id.as_deref() == Some(epic.frontmatter.id.as_str()) {
        deliverable.frontmatter.epic_id = None;
    }

    epic.frontmatter.updated = now_iso8601();
    deliverable.frontmatter.updated = now_iso8601();

    persist_task(storage, &epic).await?;
    persist_task(storage, &deliverable).await?;

    Ok(json!({"ok": true}))
}

pub async fn append_task_note(storage: &Storage, input: AppendTaskNoteInput) -> Result<Value> {
    let path = locate_task_path(storage, &input.locator).await?;
    let mut document = read_task_from_path(&path).await?;

    let use_timestamp = input.timestamp.unwrap_or(true);
    let note_text = if use_timestamp {
        let date = Local::now().format("%Y-%m-%d");
        format!("{date}: {}", input.note)
    } else {
        input.note.clone()
    };

    if let Some(section) = &input.section {
        let heading = format!("## {section}");
        if document.body.contains(&heading) {
            // Insert after the last line that belongs to the section (before next ## or end).
            let lines: Vec<&str> = document.body.lines().collect();
            let mut insert_at = None;
            let mut in_section = false;
            for (i, line) in lines.iter().enumerate() {
                if line.trim() == heading {
                    in_section = true;
                    continue;
                }
                if in_section && line.starts_with("## ") {
                    insert_at = Some(i);
                    break;
                }
            }
            let insert_at = insert_at.unwrap_or(lines.len());
            let mut new_lines = lines[..insert_at].to_vec();
            new_lines.push(&note_text);
            new_lines.extend_from_slice(&lines[insert_at..]);
            document.body = new_lines.join("\n");
        } else {
            // Section doesn't exist — create it at the end.
            if !document.body.is_empty() {
                document.body.push_str("\n\n");
            }
            document.body.push_str(&heading);
            document.body.push('\n');
            document.body.push_str(&note_text);
        }
    } else {
        if !document.body.is_empty() {
            document.body.push('\n');
        }
        document.body.push_str(&note_text);
    }

    document.frontmatter.updated = now_iso8601();
    persist_task(storage, &document).await?;

    Ok(json!({
        "id": document.frontmatter.id,
        "path": path.to_string_lossy(),
    }))
}

pub async fn add_external_ref(storage: &Storage, input: AddExternalRefInput) -> Result<Value> {
    let path = locate_task_path(storage, &input.locator).await?;
    let mut document = read_task_from_path(&path).await?;

    let new_ref = ExternalRef {
        system: input.system.clone(),
        reference: input.reference.clone(),
        url: input.url.clone(),
    };

    let mut refs = document.frontmatter.external_refs.unwrap_or_default();
    let already_exists = refs
        .iter()
        .any(|r| r.system == new_ref.system && r.reference == new_ref.reference);
    if !already_exists {
        refs.push(new_ref);
    }
    document.frontmatter.external_refs = Some(refs);
    document.frontmatter.updated = now_iso8601();

    persist_task(storage, &document).await?;

    Ok(json!({
        "id": document.frontmatter.id,
        "path": path.to_string_lossy(),
        "deduplicated": already_exists,
    }))
}

pub async fn repair_task_frontmatter(
    storage: &Storage,
    input: RepairTaskFrontmatterInput,
) -> Result<Value> {
    let path = locate_task_path(storage, &input.locator).await?;
    let raw = fs::read_to_string(&path).await?;

    // Check if it's actually broken.
    if parse_task_markdown(path.to_string_lossy().to_string(), &raw).is_ok() {
        return Ok(json!({
            "repaired": false,
            "message": "frontmatter is already valid",
            "path": path.to_string_lossy(),
        }));
    }

    let dry_run = input.dry_run.unwrap_or(false);

    // Split raw content into yaml_block and body_raw regardless of validity.
    let (yaml_block, body_raw) = split_raw_frontmatter(&raw);

    let repaired_content = match input.strategy {
        RepairStrategy::Salvage => {
            // Best-effort: move the broken YAML into the body under a recovery heading.
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.split('_').next())
                .unwrap_or("unknown")
                .to_string();
            let list = path
                .ancestors()
                .nth(3)
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let task_type = if path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                == Some("epics")
            {
                TaskType::Epic
            } else {
                TaskType::Deliverable
            };
            let title = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.split_once('_').map(|(_, rest)| rest))
                .unwrap_or("Recovered Task")
                .replace('_', " ");
            let now = now_iso8601();
            let minimal = TaskFrontmatter {
                id,
                title,
                task_type,
                status: TaskStatus::Todo,
                list,
                created: now.clone(),
                updated: now,
                epic_id: None,
                deliverable_ids: None,
                tags: None,
                priority: None,
                due: None,
                links: None,
                assignee: None,
                external_refs: None,
            };
            let recovered_section = format!(
                "\n\n## Recovered Frontmatter\n\n```yaml\n{yaml_block}\n```"
            );
            let new_body = format!("{body_raw}{recovered_section}");
            render_task_markdown(&minimal, &new_body)?
        }
        RepairStrategy::Reset => {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.split('_').next())
                .unwrap_or("unknown")
                .to_string();
            let list = path
                .ancestors()
                .nth(3)
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let task_type = if path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                == Some("epics")
            {
                TaskType::Epic
            } else {
                TaskType::Deliverable
            };
            let title = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.split_once('_').map(|(_, rest)| rest))
                .unwrap_or("Recovered Task")
                .replace('_', " ");
            let now = now_iso8601();
            let minimal = TaskFrontmatter {
                id,
                title,
                task_type,
                status: TaskStatus::Todo,
                list,
                created: now.clone(),
                updated: now,
                epic_id: None,
                deliverable_ids: None,
                tags: None,
                priority: None,
                due: None,
                links: None,
                assignee: None,
                external_refs: None,
            };
            render_task_markdown(&minimal, &body_raw)?
        }
    };

    if dry_run {
        return Ok(json!({
            "repaired": true,
            "dry_run": true,
            "preview": repaired_content,
            "path": path.to_string_lossy(),
        }));
    }

    storage.atomic_write(&path, &repaired_content).await?;

    Ok(json!({
        "repaired": true,
        "dry_run": false,
        "path": path.to_string_lossy(),
    }))
}

/// Split raw file text into `(yaml_block, body)` without requiring the YAML to be valid.
fn split_raw_frontmatter(content: &str) -> (String, String) {
    let trimmed = content.trim_start();
    let mut lines = trimmed.lines();
    // Skip opening `---`
    lines.next();
    let mut yaml_lines = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_yaml = true;
    for line in lines {
        if in_yaml && line.trim() == "---" {
            in_yaml = false;
            continue;
        }
        if in_yaml {
            yaml_lines.push(line);
        } else {
            body_lines.push(line);
        }
    }
    (yaml_lines.join("\n"), body_lines.join("\n"))
}

pub async fn read_task_from_path(path: &Path) -> Result<TaskDocument> {
    let text = fs::read_to_string(path).await?;
    parse_task_markdown(path.to_string_lossy().to_string(), &text)
}

async fn persist_task(storage: &Storage, task: &TaskDocument) -> Result<()> {
    validate_frontmatter(&task.frontmatter)?;
    let path = storage.task_file_path(
        &task.frontmatter.list,
        task.frontmatter.task_type,
        &task.frontmatter.id,
        &task.frontmatter.title,
    );
    let markdown = render_task_markdown(&task.frontmatter, &task.body)?;
    storage.atomic_write(&path, &markdown).await
}

async fn locate_task_path(storage: &Storage, locator: &TaskLocator) -> Result<PathBuf> {
    match (&locator.id, &locator.path) {
        (Some(id), None) => storage.find_task_path_by_id(id).await,
        (None, Some(path)) => Ok(PathBuf::from(path)),
        (Some(_), Some(_)) => Err(TaskMcpError::InvalidArgument(
            "provide either id or path, not both".to_string(),
        )),
        (None, None) => Err(TaskMcpError::InvalidArgument(
            "one of id or path is required".to_string(),
        )),
    }
}

fn apply_patch(document: &mut TaskDocument, patch: &Value) -> Result<()> {
    if let Some(title) = patch.get("title").and_then(Value::as_str) {
        document.frontmatter.title = title.to_string();
    }
    if let Some(status) = patch.get("status") {
        document.frontmatter.status = serde_json::from_value(status.clone())?;
    }
    if let Some(tags) = patch.get("tags") {
        document.frontmatter.tags = Some(serde_json::from_value(tags.clone())?);
    }
    if patch.get("tags").is_some_and(Value::is_null) {
        document.frontmatter.tags = None;
    }
    if let Some(priority) = patch.get("priority") {
        document.frontmatter.priority = Some(serde_json::from_value(priority.clone())?);
    }
    if patch.get("priority").is_some_and(Value::is_null) {
        document.frontmatter.priority = None;
    }
    if let Some(due) = patch.get("due").and_then(Value::as_str) {
        document.frontmatter.due = Some(due.to_string());
    }
    if patch.get("due").is_some_and(Value::is_null) {
        document.frontmatter.due = None;
    }
    if let Some(links) = patch.get("links") {
        document.frontmatter.links = Some(serde_json::from_value(links.clone())?);
    }
    if patch.get("links").is_some_and(Value::is_null) {
        document.frontmatter.links = None;
    }
    if let Some(assignee) = patch.get("assignee").and_then(Value::as_str) {
        document.frontmatter.assignee = Some(assignee.to_string());
    }
    if patch.get("assignee").is_some_and(Value::is_null) {
        document.frontmatter.assignee = None;
    }
    if let Some(epic_id) = patch.get("epic_id").and_then(Value::as_str) {
        document.frontmatter.epic_id = Some(epic_id.to_string());
    }
    if patch.get("epic_id").is_some_and(Value::is_null) {
        document.frontmatter.epic_id = None;
    }
    if let Some(deliverable_ids) = patch.get("deliverable_ids") {
        document.frontmatter.deliverable_ids =
            Some(serde_json::from_value(deliverable_ids.clone())?);
    }
    if patch.get("deliverable_ids").is_some_and(Value::is_null) {
        document.frontmatter.deliverable_ids = None;
    }
    if let Some(body) = patch.get("body").and_then(Value::as_str) {
        document.body = body.to_string();
    }
    if let Some(append) = patch.get("body_append").and_then(Value::as_str) {
        if !document.body.is_empty() {
            document.body.push('\n');
        }
        document.body.push_str(append);
    }
    if let Some(prepend) = patch.get("body_prepend").and_then(Value::as_str) {
        document.body = format!("{prepend}\n{}", document.body);
    }

    Ok(())
}
