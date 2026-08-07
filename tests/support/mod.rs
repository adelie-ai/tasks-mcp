//! Shared driver for the mcp-core#40 content-leak tests.
//!
//! One scripted flow calls every MCP tool this server exposes, with a
//! distinguishable sentinel planted in each tool's most content-bearing
//! field. `telemetry_span_fields.rs` (in-process) and `telemetry_console.rs`
//! (over stdio) each replay this same flow through their own [`Transport`],
//! so the two content tests check the same surface rather than two
//! different, narrower ones.
//!
//! Why a shared, scripted flow rather than one sentinel on one tool: a test
//! that only calls `create_task` proves the mechanism works for
//! `create_task`. It says nothing about `append_task_note` or
//! `add_external_ref`. The coverage check in each test file compares what
//! this flow exercised against the live [`tasks_mcp::tools::tool_definitions`]
//! list, so a fifteenth tool added later and left out of this flow fails the
//! test that should have covered it.
#![allow(dead_code)]

use serde_json::{Value, json};

/// A fixed, distinguishable prefix. ASCII alphanumerics, hyphens and
/// underscores only, so it doubles as a valid list name
/// (`validate_list_name` in `src/storage.rs`).
pub const SENTINEL_BASE: &str = "sentinel-8f3a1c2e-leak-check";

/// A per-tool sentinel, so a leak names which tool produced it.
pub fn tag(tool: &str) -> String {
    format!("{SENTINEL_BASE}-{tool}")
}

/// One request/response round trip, or a fire-and-forget notification, over
/// whatever transport a test wires up (in-process `Session`, or a real
/// stdio child process).
pub trait Transport {
    /// Send a request and return its response body (the whole JSON-RPC
    /// envelope, so a caller can read `result` or `error`).
    async fn call(&mut self, id: u64, method: &str, params: Value) -> Value;
    /// Send a notification. No response is expected.
    async fn notify(&mut self, method: &str, params: Value);
}

/// What the flow exercised, for the coverage and leak checks each test file
/// runs afterward.
pub struct FlowResult {
    /// Every tool name the flow called at least once.
    pub exercised_tools: Vec<String>,
    /// Every sentinel value the flow planted in a tool argument, across every
    /// call. A content test asserts none of these ever reaches a span field
    /// or an INFO-or-louder log line.
    pub sentinels: Vec<String>,
}

fn structured_content(response: &Value) -> Value {
    response
        .get("result")
        .and_then(|v| v.get("structuredContent"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn tool_call(name: &str, arguments: Value) -> Value {
    json!({ "name": name, "arguments": arguments })
}

/// Call every tool `tasks_mcp::tools::tool_definitions()` advertises, in an
/// order that satisfies each tool's own data dependencies (a task must exist
/// before it can be fetched, linked or deleted), planting a distinct
/// sentinel in each call that takes a content-bearing argument.
///
/// Two tools take no free-text argument at all (`list_lists`,
/// `repair_task_frontmatter`'s locator/strategy/dry_run are ids and enums,
/// not content) and so plant no sentinel; they are still called, so the
/// coverage check still requires them present.
pub async fn run_content_leak_flow(transport: &mut impl Transport) -> FlowResult {
    let mut exercised = Vec::new();
    let mut sentinels = Vec::new();
    let mut next_id: u64 = 1;
    macro_rules! next {
        () => {{
            let id = next_id;
            next_id += 1;
            id
        }};
    }

    transport
        .call(
            next!(),
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "content-leak-test", "version": "0.1.0"}
            }),
        )
        .await;
    transport
        .notify("notifications/initialized", json!({}))
        .await;

    let list_name = tag("create_list");
    sentinels.push(list_name.clone());

    transport
        .call(
            next!(),
            "tools/call",
            tool_call("create_list", json!({"name": list_name})),
        )
        .await;
    exercised.push("create_list".to_string());

    transport
        .call(next!(), "tools/call", tool_call("list_lists", json!({})))
        .await;
    exercised.push("list_lists".to_string());

    let epic_title = tag("create_task-epic");
    sentinels.push(epic_title.clone());
    let epic_resp = transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "create_task",
                json!({"list": list_name, "type": "epic", "title": epic_title}),
            ),
        )
        .await;
    exercised.push("create_task".to_string());
    let epic_id = structured_content(&epic_resp)["id"]
        .as_str()
        .expect("epic id present")
        .to_string();

    let deliverable_title = tag("create_task-deliverable");
    sentinels.push(deliverable_title.clone());
    let deliverable_resp = transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "create_task",
                json!({"list": list_name, "type": "deliverable", "title": deliverable_title}),
            ),
        )
        .await;
    let deliverable_id = structured_content(&deliverable_resp)["id"]
        .as_str()
        .expect("deliverable id present")
        .to_string();

    let disposable_title = tag("delete_task-target");
    sentinels.push(disposable_title.clone());
    let disposable_resp = transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "create_task",
                json!({"list": list_name, "type": "deliverable", "title": disposable_title}),
            ),
        )
        .await;
    let disposable_id = structured_content(&disposable_resp)["id"]
        .as_str()
        .expect("disposable id present")
        .to_string();

    transport
        .call(
            next!(),
            "tools/call",
            tool_call("get_task", json!({"id": deliverable_id})),
        )
        .await;
    exercised.push("get_task".to_string());

    let update_note = tag("update_task");
    sentinels.push(update_note.clone());
    transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "update_task",
                json!({"id": deliverable_id, "patch": {"body_append": update_note}}),
            ),
        )
        .await;
    exercised.push("update_task".to_string());

    transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "set_status",
                json!({"id": deliverable_id, "status": "doing"}),
            ),
        )
        .await;
    exercised.push("set_status".to_string());

    transport
        .call(
            next!(),
            "tools/call",
            tool_call("list_tasks", json!({"lists": [list_name]})),
        )
        .await;
    exercised.push("list_tasks".to_string());

    let search_text = tag("search_tasks");
    sentinels.push(search_text.clone());
    transport
        .call(
            next!(),
            "tools/call",
            tool_call("search_tasks", json!({"text": search_text})),
        )
        .await;
    exercised.push("search_tasks".to_string());

    transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "add_deliverable",
                json!({"epic_id": epic_id, "deliverable_id": deliverable_id}),
            ),
        )
        .await;
    exercised.push("add_deliverable".to_string());

    transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "remove_deliverable",
                json!({"epic_id": epic_id, "deliverable_id": deliverable_id}),
            ),
        )
        .await;
    exercised.push("remove_deliverable".to_string());

    let note = tag("append_task_note");
    sentinels.push(note.clone());
    transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "append_task_note",
                json!({"id": deliverable_id, "note": note}),
            ),
        )
        .await;
    exercised.push("append_task_note".to_string());

    let external_ref = tag("add_external_ref");
    sentinels.push(external_ref.clone());
    transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "add_external_ref",
                json!({"id": deliverable_id, "system": "github", "ref": external_ref}),
            ),
        )
        .await;
    exercised.push("add_external_ref".to_string());

    transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "repair_task_frontmatter",
                json!({"id": deliverable_id, "strategy": "salvage", "dry_run": true}),
            ),
        )
        .await;
    exercised.push("repair_task_frontmatter".to_string());

    transport
        .call(
            next!(),
            "tools/call",
            tool_call("delete_task", json!({"id": disposable_id})),
        )
        .await;
    exercised.push("delete_task".to_string());

    // ---- failure paths (lesson 9) ---------------------------------------
    //
    // A content leak shows up most naturally in the error branch: an error
    // type's `Display` is written to be helpful, and helpful means quoting
    // back whatever failed. Driving only the success path -- as the first
    // version of this flow did -- never runs that code at all. Two of the
    // calls below are chosen because the underlying implementation is known
    // to build its message with `format!`, quoting the argument back
    // (`src/operations/task_ops.rs`'s "outside the tasks root" check); the
    // rest exercise `NotFound`, so every failure-classification path
    // (`CallError::Tool`, `CallError::InvalidParams`) actually runs under
    // this flow, not only `Ok`.

    let missing_id = tag("not-found-id");
    sentinels.push(missing_id.clone());
    for (tool, arguments) in [
        ("get_task", json!({"id": missing_id})),
        ("update_task", json!({"id": missing_id, "patch": {}})),
        ("set_status", json!({"id": missing_id, "status": "doing"})),
        ("append_task_note", json!({"id": missing_id, "note": "x"})),
        (
            "add_external_ref",
            json!({"id": missing_id, "system": "github", "ref": "x"}),
        ),
        (
            "repair_task_frontmatter",
            json!({"id": missing_id, "strategy": "salvage"}),
        ),
        (
            "add_deliverable",
            json!({"epic_id": missing_id, "deliverable_id": deliverable_id}),
        ),
        (
            "remove_deliverable",
            json!({"epic_id": missing_id, "deliverable_id": deliverable_id}),
        ),
    ] {
        let response = transport
            .call(next!(), "tools/call", tool_call(tool, arguments))
            .await;
        assert_failed(&response, tool);
    }

    // The path-outside-root check quotes the candidate path back into the
    // error message (`format!("path '{}' is outside the tasks root ...")`),
    // which is exactly the shape that leaked a URL in one sibling server and
    // a path in another.
    let outside_root_sentinel = tag("outside-root-path");
    sentinels.push(outside_root_sentinel.clone());
    let outside_path = format!("/tmp/{outside_root_sentinel}-outside-root.md");
    let response = transport
        .call(
            next!(),
            "tools/call",
            tool_call("get_task", json!({"path": outside_path})),
        )
        .await;
    assert_failed(&response, "get_task (path outside root)");

    // `create_list` fails its charset check (a `/` is not a valid list-name
    // character), driving `InvalidArgument` from a different call site.
    let invalid_list_sentinel = tag("create_list-invalid");
    sentinels.push(invalid_list_sentinel.clone());
    let bad_list_name = format!("{invalid_list_sentinel}/../invalid");
    let response = transport
        .call(
            next!(),
            "tools/call",
            tool_call("create_list", json!({"name": bad_list_name})),
        )
        .await;
    assert_failed(&response, "create_list (invalid name)");

    // An unrecognised `type` fails argument deserialization, driving
    // `CallError::InvalidParams` -- a different mcp-core classification
    // branch than the domain-error `CallError::Tool` cases above.
    let bad_type_sentinel = tag("create_task-invalid-type");
    sentinels.push(bad_type_sentinel.clone());
    let response = transport
        .call(
            next!(),
            "tools/call",
            tool_call(
                "create_task",
                json!({"list": list_name, "type": bad_type_sentinel, "title": "x"}),
            ),
        )
        .await;
    assert_failed(&response, "create_task (invalid type)");

    // The last `next!()` above increments `next_id` one final time with
    // nothing left to read it; note that explicitly rather than leave a dead
    // store the lint would otherwise (correctly) flag.
    let _ = next_id;

    FlowResult {
        exercised_tools: exercised,
        sentinels,
    }
}

/// Assert that a `tools/call` response is a failure (either `isError: true`
/// tool-level content, or a JSON-RPC protocol `error`) -- so a failure-path
/// step that accidentally started succeeding is caught here, rather than
/// silently skipping the error branch it exists to exercise.
fn assert_failed(response: &Value, label: &str) {
    let is_tool_error = response
        .get("result")
        .and_then(|r| r.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_protocol_error = response.get("error").is_some();
    assert!(
        is_tool_error || is_protocol_error,
        "{label} was expected to fail so its error branch runs under this flow, \
         but it succeeded: {response}"
    );
}
