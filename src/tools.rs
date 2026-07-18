#![deny(warnings)]

use mcp_core::ToolDef;
use serde_json::{Value, json};

use crate::error::{Result, TaskMcpError};
use crate::operations::task_ops::{
    AddExternalRefInput, AppendTaskNoteInput, CreateTaskInput, DeleteTaskInput, ListTasksInput,
    RelationshipInput, RepairTaskFrontmatterInput, SearchTasksInput, SetStatusInput, TaskLocator,
    UpdateTaskInput, add_deliverable, add_external_ref, append_task_note, create_task, delete_task,
    get_task, list_tasks, remove_deliverable, repair_task_frontmatter, search_tasks, set_status,
};
use crate::storage::Storage;

pub fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef::new(
            "list_lists",
            "List available task lists/contexts.",
            json!({"type":"object","properties":{}}),
        ),
        ToolDef::new(
            "create_list",
            "Create a new task list with epics and deliverables directories.",
            json!({
                "type":"object",
                "properties":{"name":{"type":"string"}},
                "required":["name"]
            }),
        ),
        ToolDef::new(
            "create_task",
            "Create a new task markdown file.",
            json!({
                "type":"object",
                "properties": {
                    "list": {"type":"string"},
                    "type": {"type":"string","enum":["epic","deliverable"]},
                    "title": {"type":"string"},
                    "status": {"type":"string","enum":["todo","doing","blocked","validating","done","canceled"]},
                    "epic_id": {"type":"string"},
                    "deliverable_ids": {"type":"array","items":{"type":"string"}},
                    "tags": {"type":"array","items":{"type":"string"}},
                    "priority": {"type":"string","enum":["p0","p1","p2","p3"]},
                    "due": {"type":"string"},
                    "links": {"type":"array","items":{"type":"string"}},
                    "assignee": {"type":"string"},
                    "body": {"type":"string"}
                },
                "required": ["list", "type", "title"]
            }),
        ),
        ToolDef::new(
            "get_task",
            "Get a task by id or path.",
            json!({
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"}
                }
            }),
        ),
        ToolDef::new(
            "update_task",
            "Update frontmatter/body fields and refresh updated timestamp. Use body_append or body_prepend to safely add text without replacing the full body.",
            json!({
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "patch": {
                        "type":"object",
                        "description": "Fields to update. All fields are optional; omit fields that should not change. Set a nullable field to null to clear it.",
                        "properties": {
                            "title": {"type":"string","description":"New task title."},
                            "status": {"type":"string","enum":["todo","doing","blocked","validating","done","canceled"],"description":"New task status."},
                            "tags": {"type":["array","null"],"items":{"type":"string"},"description":"Replace the tag list (null to clear)."},
                            "priority": {"type":["string","null"],"enum":["low","medium","high","critical",null],"description":"Task priority (null to clear)."},
                            "due": {"type":["string","null"],"description":"Due date as ISO 8601 string (null to clear)."},
                            "links": {"type":["array","null"],"items":{"type":"string"},"description":"Replace the links list (null to clear)."},
                            "assignee": {"type":["string","null"],"description":"Assignee identifier (null to clear)."},
                            "epic_id": {"type":["string","null"],"description":"Parent epic ID (null to clear)."},
                            "deliverable_ids": {"type":["array","null"],"items":{"type":"string"},"description":"Replace the deliverable IDs list (null to clear)."},
                            "body": {"type":"string","description":"Replace the entire task body."},
                            "body_append": {"type":"string","description":"Text to append to the end of the task body."},
                            "body_prepend": {"type":"string","description":"Text to prepend to the start of the task body."}
                        }
                    }
                },
                "required": ["patch"]
            }),
        ),
        ToolDef::new(
            "set_status",
            "Set task status directly. Valid statuses: todo, doing, blocked, validating, done, canceled.",
            json!({
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "status": {"type":"string","enum":["todo","doing","blocked","validating","done","canceled"]}
                },
                "required": ["status"]
            }),
        ),
        ToolDef::new(
            "delete_task",
            "Delete a task by id or path.",
            json!({
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"}
                }
            }),
        ),
        ToolDef::new(
            "list_tasks",
            "List tasks across all lists or within a provided list subset.",
            json!({
                "type":"object",
                "properties": {
                    "list": {"type":"string"},
                    "lists": {"type":"array","items":{"type":"string"}},
                    "type": {"type":"string","enum":["epic","deliverable"]},
                    "status": {"type":"string","enum":["todo","doing","blocked","validating","done","canceled"]},
                    "tag": {"type":"string"},
                    "assignee": {"type":"string"},
                    "epic_id": {"type":"string"}
                }
            }),
        ),
        ToolDef::new(
            "search_tasks",
            "Search task titles and bodies.",
            json!({
                "type":"object",
                "properties": {
                    "text": {"type":"string"},
                    "lists": {"type":"array","items":{"type":"string"}}
                },
                "required": ["text"]
            }),
        ),
        ToolDef::new(
            "add_deliverable",
            "Link a deliverable to an epic and keep both sides in sync.",
            json!({
                "type":"object",
                "properties": {
                    "epic_id": {"type":"string"},
                    "deliverable_id": {"type":"string"}
                },
                "required": ["epic_id", "deliverable_id"]
            }),
        ),
        ToolDef::new(
            "remove_deliverable",
            "Unlink a deliverable from an epic.",
            json!({
                "type":"object",
                "properties": {
                    "epic_id": {"type":"string"},
                    "deliverable_id": {"type":"string"}
                },
                "required": ["epic_id", "deliverable_id"]
            }),
        ),
        ToolDef::new(
            "append_task_note",
            "Append a freeform note to the task body without touching frontmatter. Safely handles Markdown special characters. Optionally insert under a named heading.",
            json!({
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "note": {"type":"string"},
                    "section": {"type":"string","description":"Heading to insert the note under (e.g. 'Notes'). Created if absent."},
                    "timestamp": {"type":"boolean","description":"Prefix note with today's date (default: true)."}
                },
                "required": ["note"]
            }),
        ),
        ToolDef::new(
            "add_external_ref",
            "Add a structured external ticket reference (e.g. Jira, GitHub) to a task's frontmatter. Deduplicates by system+ref.",
            json!({
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "system": {"type":"string","description":"Ticket system identifier, e.g. 'jira', 'github'."},
                    "ref": {"type":"string","description":"The ticket/issue reference, e.g. 'PROJ-123'."},
                    "url": {"type":"string","description":"Optional URL to the ticket."}
                },
                "required": ["system", "ref"]
            }),
        ),
        ToolDef::new(
            "repair_task_frontmatter",
            "Repair a task whose YAML frontmatter has become invalid. Use after corruption (e.g. from raw file edits).",
            json!({
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "strategy": {"type":"string","enum":["salvage","reset"],"description":"salvage: move broken YAML to body under ## Recovered Frontmatter; reset: rewrite frontmatter from file path metadata."},
                    "dry_run": {"type":"boolean","description":"Return repaired content without writing to disk (default: false)."}
                },
                "required": ["strategy"]
            }),
        ),
    ]
}

#[cfg(test)]
mod description_tests {
    use super::tool_definitions;

    fn description_for(name: &str) -> String {
        tool_definitions()
            .into_iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("tool `{name}` must be defined"))
            .description
            .to_lowercase()
    }

    /// `create_task` should read as "add a to-do / work item", not
    /// "create a markdown file" (mechanism-first).
    #[test]
    fn create_task_description_leads_with_purpose() {
        let desc = description_for("create_task");
        assert!(
            desc.contains("to-do") || desc.contains("work item"),
            "create_task should describe adding a to-do / work item, got: {desc}"
        );
    }

    /// `update_task` should read as "edit a task's fields", not
    /// "update frontmatter/body fields" (mechanism-first), while keeping the
    /// body_append / body_prepend guidance.
    #[test]
    fn update_task_description_leads_with_purpose() {
        let desc = description_for("update_task");
        assert!(
            desc.contains("edit a task"),
            "update_task should lead with editing a task's fields, got: {desc}"
        );
        assert!(
            desc.contains("body_append") && desc.contains("body_prepend"),
            "update_task must keep the safe-append guidance, got: {desc}"
        );
    }

    /// `list_tasks` should surface its filtering power so the model knows it
    /// can narrow by status, tag, assignee, etc.
    #[test]
    fn list_tasks_description_advertises_filters() {
        let desc = description_for("list_tasks");
        assert!(
            desc.contains("filter"),
            "list_tasks should advertise filtering, got: {desc}"
        );
        for term in ["status", "tag", "assignee"] {
            assert!(
                desc.contains(term),
                "list_tasks should mention the `{term}` filter, got: {desc}"
            );
        }
    }
}

pub async fn call_tool(storage: &Storage, name: &str, arguments: Value) -> Result<Value> {
    match name {
        "list_lists" => {
            let lists = storage.list_lists().await?;
            Ok(json!(lists))
        }
        "create_list" => {
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| TaskMcpError::InvalidArgument("name is required".to_string()))?;
            storage.create_list(name).await?;
            Ok(json!({"created": true, "name": name}))
        }
        "create_task" => {
            let input: CreateTaskInput = serde_json::from_value(arguments)?;
            create_task(storage, input).await
        }
        "get_task" => {
            let locator: TaskLocator = serde_json::from_value(arguments)?;
            get_task(storage, locator).await
        }
        "update_task" => {
            let input: UpdateTaskInput = serde_json::from_value(arguments)?;
            crate::operations::task_ops::update_task(storage, input).await
        }
        "set_status" => {
            let input: SetStatusInput = serde_json::from_value(arguments)?;
            set_status(storage, input).await
        }
        "delete_task" => {
            let input: DeleteTaskInput = serde_json::from_value(arguments)?;
            delete_task(storage, input).await
        }
        "list_tasks" => {
            let input: ListTasksInput = serde_json::from_value(arguments)?;
            list_tasks(storage, input).await
        }
        "search_tasks" => {
            let input: SearchTasksInput = serde_json::from_value(arguments)?;
            search_tasks(storage, input).await
        }
        "add_deliverable" => {
            let input: RelationshipInput = serde_json::from_value(arguments)?;
            add_deliverable(storage, input).await
        }
        "remove_deliverable" => {
            let input: RelationshipInput = serde_json::from_value(arguments)?;
            remove_deliverable(storage, input).await
        }
        "append_task_note" => {
            let input: AppendTaskNoteInput = serde_json::from_value(arguments)?;
            append_task_note(storage, input).await
        }
        "add_external_ref" => {
            let input: AddExternalRefInput = serde_json::from_value(arguments)?;
            add_external_ref(storage, input).await
        }
        "repair_task_frontmatter" => {
            let input: RepairTaskFrontmatterInput = serde_json::from_value(arguments)?;
            repair_task_frontmatter(storage, input).await
        }
        _ => Err(TaskMcpError::NotFound(format!("unknown tool: {name}"))),
    }
}
