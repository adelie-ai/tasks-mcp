#![deny(warnings)]

use serde_json::{Value, json};

use crate::error::{Result, TaskMcpError};
use crate::operations::task_ops::{
    AddExternalRefInput, AppendTaskNoteInput, CreateTaskInput, DeleteTaskInput, ListTasksInput,
    RelationshipInput, RepairTaskFrontmatterInput, SearchTasksInput, SetStatusInput, TaskLocator,
    UpdateTaskInput, add_deliverable, add_external_ref, append_task_note, create_task, delete_task,
    get_task, list_tasks, remove_deliverable, repair_task_frontmatter, search_tasks, set_status,
};
use crate::server::McpServer;

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "list_lists",
            "description": "List available task lists/contexts.",
            "inputSchema": {"type":"object","properties":{}}
        }),
        json!({
            "name": "create_list",
            "description": "Create a new task list with epics and deliverables directories.",
            "inputSchema": {
                "type":"object",
                "properties":{"name":{"type":"string"}},
                "required":["name"]
            }
        }),
        json!({
            "name": "create_task",
            "description": "Create a new task markdown file.",
            "inputSchema": {
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
            }
        }),
        json!({
            "name": "get_task",
            "description": "Get a task by id or path.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"}
                }
            }
        }),
        json!({
            "name": "update_task",
            "description": "Update frontmatter/body fields and refresh updated timestamp. Use body_append or body_prepend to safely add text without replacing the full body.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "patch": {
                        "type":"object",
                        "properties": {
                            "body_append": {"type":"string","description":"Text to append to the end of the task body."},
                            "body_prepend": {"type":"string","description":"Text to prepend to the start of the task body."}
                        }
                    }
                },
                "required": ["patch"]
            }
        }),
        json!({
            "name": "set_status",
            "description": "Set task status directly. Valid statuses: todo, doing, blocked, validating, done, canceled.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "status": {"type":"string","enum":["todo","doing","blocked","validating","done","canceled"]}
                },
                "required": ["status"]
            }
        }),
        json!({
            "name": "delete_task",
            "description": "Delete a task by id or path.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"}
                }
            }
        }),
        json!({
            "name": "list_tasks",
            "description": "List tasks across all lists or within a provided list subset.",
            "inputSchema": {
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
            }
        }),
        json!({
            "name": "search_tasks",
            "description": "Search task titles and bodies.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "text": {"type":"string"},
                    "lists": {"type":"array","items":{"type":"string"}}
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "add_deliverable",
            "description": "Link a deliverable to an epic and keep both sides in sync.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "epic_id": {"type":"string"},
                    "deliverable_id": {"type":"string"}
                },
                "required": ["epic_id", "deliverable_id"]
            }
        }),
        json!({
            "name": "remove_deliverable",
            "description": "Unlink a deliverable from an epic.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "epic_id": {"type":"string"},
                    "deliverable_id": {"type":"string"}
                },
                "required": ["epic_id", "deliverable_id"]
            }
        }),
        json!({
            "name": "append_task_note",
            "description": "Append a freeform note to the task body without touching frontmatter. Safely handles Markdown special characters. Optionally insert under a named heading.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "note": {"type":"string"},
                    "section": {"type":"string","description":"Heading to insert the note under (e.g. 'Notes'). Created if absent."},
                    "timestamp": {"type":"boolean","description":"Prefix note with today's date (default: true)."}
                },
                "required": ["note"]
            }
        }),
        json!({
            "name": "add_external_ref",
            "description": "Add a structured external ticket reference (e.g. Jira, GitHub) to a task's frontmatter. Deduplicates by system+ref.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "system": {"type":"string","description":"Ticket system identifier, e.g. 'jira', 'github'."},
                    "ref": {"type":"string","description":"The ticket/issue reference, e.g. 'PROJ-123'."},
                    "url": {"type":"string","description":"Optional URL to the ticket."}
                },
                "required": ["system", "ref"]
            }
        }),
        json!({
            "name": "repair_task_frontmatter",
            "description": "Repair a task whose YAML frontmatter has become invalid. Use after corruption (e.g. from raw file edits).",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "strategy": {"type":"string","enum":["salvage","reset"],"description":"salvage: move broken YAML to body under ## Recovered Frontmatter; reset: rewrite frontmatter from file path metadata."},
                    "dry_run": {"type":"boolean","description":"Return repaired content without writing to disk (default: false)."}
                },
                "required": ["strategy"]
            }
        }),
    ]
}

pub async fn call_tool(server: &McpServer, name: &str, arguments: Value) -> Result<Value> {
    match name {
        "list_lists" => {
            let lists = server.storage().list_lists().await?;
            Ok(json!(lists))
        }
        "create_list" => {
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| TaskMcpError::InvalidArgument("name is required".to_string()))?;
            server.storage().create_list(name).await?;
            Ok(json!({"created": true, "name": name}))
        }
        "create_task" => {
            let input: CreateTaskInput = serde_json::from_value(arguments)?;
            create_task(server.storage(), input).await
        }
        "get_task" => {
            let locator: TaskLocator = serde_json::from_value(arguments)?;
            get_task(server.storage(), locator).await
        }
        "update_task" => {
            let input: UpdateTaskInput = serde_json::from_value(arguments)?;
            crate::operations::task_ops::update_task(server.storage(), input).await
        }
        "set_status" => {
            let input: SetStatusInput = serde_json::from_value(arguments)?;
            set_status(server.storage(), input).await
        }
        "delete_task" => {
            let input: DeleteTaskInput = serde_json::from_value(arguments)?;
            delete_task(server.storage(), input).await
        }
        "list_tasks" => {
            let input: ListTasksInput = serde_json::from_value(arguments)?;
            list_tasks(server.storage(), input).await
        }
        "search_tasks" => {
            let input: SearchTasksInput = serde_json::from_value(arguments)?;
            search_tasks(server.storage(), input).await
        }
        "add_deliverable" => {
            let input: RelationshipInput = serde_json::from_value(arguments)?;
            add_deliverable(server.storage(), input).await
        }
        "remove_deliverable" => {
            let input: RelationshipInput = serde_json::from_value(arguments)?;
            remove_deliverable(server.storage(), input).await
        }
        "append_task_note" => {
            let input: AppendTaskNoteInput = serde_json::from_value(arguments)?;
            append_task_note(server.storage(), input).await
        }
        "add_external_ref" => {
            let input: AddExternalRefInput = serde_json::from_value(arguments)?;
            add_external_ref(server.storage(), input).await
        }
        "repair_task_frontmatter" => {
            let input: RepairTaskFrontmatterInput = serde_json::from_value(arguments)?;
            repair_task_frontmatter(server.storage(), input).await
        }
        _ => Err(TaskMcpError::NotFound(format!("unknown tool: {name}"))),
    }
}
