use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;

struct StdioMcpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl StdioMcpClient {
    fn spawn(tasks_root: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tasks-mcp"))
            .arg("serve")
            .arg("--mode")
            .arg("stdio")
            .env("TASKS_MCP_ROOT", tasks_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn tasks-mcp");

        let stdin = child.stdin.take().expect("take child stdin");
        let stdout = child.stdout.take().expect("take child stdout");

        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        }
    }

    fn send(&mut self, value: &Value) {
        let encoded = serde_json::to_string(value).expect("serialize request");
        self.stdin
            .write_all(encoded.as_bytes())
            .expect("write request");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush request");
    }

    fn receive(&mut self) -> Value {
        let mut line = String::new();
        let bytes = self
            .stdout
            .read_line(&mut line)
            .expect("read response line");
        assert!(bytes > 0, "server closed stdout before sending response");
        serde_json::from_str(line.trim_end()).expect("parse JSON response")
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        self.receive()
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn structured_content(response: &Value) -> &Value {
    response
        .get("result")
        .and_then(|v| v.get("structuredContent"))
        .expect("structuredContent present")
}

fn tool_call_is_error(response: &Value) -> bool {
    response
        .get("result")
        .and_then(|v| v.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn test_temp_root() -> TempDir {
    tempfile::tempdir().expect("create temp root")
}

#[test]
fn tools_list_requires_initialized_notification() {
    let root = test_temp_root();
    let mut client = StdioMcpClient::spawn(root.path());

    let response = client.request(1, "tools/list", json!({}));

    let code = response
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(Value::as_i64);
    assert_eq!(code, Some(-32000));
}

#[test]
fn stdio_end_to_end_mcp_flow() {
    let root = test_temp_root();
    let mut client = StdioMcpClient::spawn(root.path());

    let initialize = client.request(
        1,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1.0"}
        }),
    );
    assert_eq!(
        initialize
            .get("result")
            .and_then(|v| v.get("serverInfo"))
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str),
        Some("tasks-mcp")
    );

    client.notify("initialized", json!({}));

    let tools = client.request(2, "tools/list", json!({}));
    let tool_names: Vec<&str> = tools
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(Value::as_array)
        .expect("tools array")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();
    assert!(tool_names.contains(&"create_task"));
    assert!(tool_names.contains(&"list_tasks"));

    let create_list = client.request(
        3,
        "tools/call",
        json!({
            "name": "create_list",
            "arguments": {"name": "project-alpha"}
        }),
    );
    assert_eq!(
        structured_content(&create_list)
            .get("created")
            .and_then(Value::as_bool),
        Some(true)
    );

    let create_task = client.request(
        4,
        "tools/call",
        json!({
            "name": "create_task",
            "arguments": {
                "list": "project-alpha",
                "type": "deliverable",
                "title": "Request environment access",
                "status": "todo",
                "tags": ["access", "environment"],
                "body": "## Checklist\n- [ ] Submit request"
            }
        }),
    );

    let task_id = structured_content(&create_task)
        .get("id")
        .and_then(Value::as_str)
        .expect("created id")
        .to_string();

    let list_tasks = client.request(
        5,
        "tools/call",
        json!({
            "name": "list_tasks",
            "arguments": {"lists": ["project-alpha"]}
        }),
    );
    let tasks = structured_content(&list_tasks)
        .as_array()
        .expect("list_tasks array");
    assert_eq!(tasks.len(), 1);

    let get_task = client.request(
        6,
        "tools/call",
        json!({
            "name": "get_task",
            "arguments": {"id": task_id}
        }),
    );
    assert_eq!(
        structured_content(&get_task)
            .get("frontmatter")
            .and_then(|v| v.get("title"))
            .and_then(Value::as_str),
        Some("Request environment access")
    );

    let shutdown = client.request(7, "shutdown", json!({}));
    assert!(shutdown.get("result").is_some());

    let deliverables_dir = root.path().join("project-alpha").join("deliverables");
    let entries = std::fs::read_dir(&deliverables_dir)
        .expect("read deliverables dir")
        .collect::<std::io::Result<Vec<_>>>()
        .expect("collect deliverables");
    assert_eq!(entries.len(), 1);
}

#[test]
fn add_deliverable_conflict_when_already_assigned() {
    let root = test_temp_root();
    let mut client = StdioMcpClient::spawn(root.path());

    let _ = client.request(
        1,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1.0"}
        }),
    );
    client.notify("initialized", json!({}));

    let _ = client.request(
        2,
        "tools/call",
        json!({
            "name": "create_list",
            "arguments": {"name": "project-alpha"}
        }),
    );

    let epic_1 = client.request(
        3,
        "tools/call",
        json!({
            "name": "create_task",
            "arguments": {
                "list": "project-alpha",
                "type": "epic",
                "title": "Epic one"
            }
        }),
    );
    let epic_1_id = structured_content(&epic_1)
        .get("id")
        .and_then(Value::as_str)
        .expect("epic 1 id")
        .to_string();

    let epic_2 = client.request(
        4,
        "tools/call",
        json!({
            "name": "create_task",
            "arguments": {
                "list": "project-alpha",
                "type": "epic",
                "title": "Epic two"
            }
        }),
    );
    let epic_2_id = structured_content(&epic_2)
        .get("id")
        .and_then(Value::as_str)
        .expect("epic 2 id")
        .to_string();

    let deliverable = client.request(
        5,
        "tools/call",
        json!({
            "name": "create_task",
            "arguments": {
                "list": "project-alpha",
                "type": "deliverable",
                "title": "Shared deliverable"
            }
        }),
    );
    let deliverable_id = structured_content(&deliverable)
        .get("id")
        .and_then(Value::as_str)
        .expect("deliverable id")
        .to_string();

    let link_first = client.request(
        6,
        "tools/call",
        json!({
            "name": "add_deliverable",
            "arguments": {
                "epic_id": epic_1_id,
                "deliverable_id": deliverable_id
            }
        }),
    );
    assert!(!tool_call_is_error(&link_first));

    let link_conflict = client.request(
        7,
        "tools/call",
        json!({
            "name": "add_deliverable",
            "arguments": {
                "epic_id": epic_2_id,
                "deliverable_id": structured_content(&deliverable)
                    .get("id")
                    .and_then(Value::as_str)
                    .expect("deliverable id present")
            }
        }),
    );

    assert!(tool_call_is_error(&link_conflict));
    let error_text = link_conflict
        .get("result")
        .and_then(|v| v.get("content"))
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(error_text.contains("already assigned to epic"));
}
