#![deny(warnings)]

use serde_json::{Value, json};

use crate::error::{Result, TaskMcpError};
use crate::operations::task_ops::{
    CreateTaskInput, DeleteTaskInput, ListTasksInput, RelationshipInput, SearchTasksInput,
    SetStatusInput, TaskLocator, UpdateTaskInput, add_deliverable, create_task, delete_task,
    get_task, list_tasks, remove_deliverable, search_tasks, set_status,
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
                    "status": {"type":"string","enum":["todo","doing","blocked","done","canceled"]},
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
            "description": "Update frontmatter/body fields and refresh updated timestamp.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "patch": {"type":"object"}
                },
                "required": ["patch"]
            }
        }),
        json!({
            "name": "set_status",
            "description": "Set task status directly. Valid statuses: todo, doing, blocked, done, canceled.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "id": {"type":"string"},
                    "path": {"type":"string"},
                    "status": {"type":"string","enum":["todo","doing","blocked","done","canceled"]}
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
                    "status": {"type":"string","enum":["todo","doing","blocked","done","canceled"]},
                    "tag": {"type":"string"},
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
        _ => Err(TaskMcpError::NotFound(format!("unknown tool: {name}"))),
    }
}
