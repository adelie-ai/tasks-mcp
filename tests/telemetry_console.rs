//! Console test: stdout carries only JSON-RPC, and no sentinel from
//! `support::run_content_leak_flow` reaches an INFO-or-louder stderr line,
//! even at `RUST_LOG=trace` (adelie-ai/mcp-core#40).
//!
//! This test and `telemetry_span_fields.rs` are deliberately different
//! shapes for the same claim (lesson 7): a span records its fields at
//! creation and nothing prints them unless an event fires inside the span,
//! so a console-text test cannot see a span-field leak. What it *can* catch,
//! and the in-process test cannot, is a stray `println!`/`eprintln!` landing
//! on the wrong stream. Keep both.
//!
//! Lesson 8: the flow drives every tool `tool_definitions()` advertises, not
//! one, so a leak on any tool's argument shows up here regardless of which
//! tool it was added to.

#![deny(warnings)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::Value;
use support::Transport;
use tempfile::TempDir;

mod support;

struct SubprocessTransport {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stdout_lines: Vec<String>,
    stderr_buffer: Arc<Mutex<String>>,
    stderr_reader: Option<JoinHandle<()>>,
}

impl SubprocessTransport {
    fn spawn(tasks_root: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tasks-mcp"))
            .args(["serve", "--mode", "stdio", "--no-dbus"])
            .env("TASKS_MCP_ROOT", tasks_root)
            .env("RUST_LOG", "trace")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn tasks-mcp");

        let stdin = child.stdin.take().expect("take child stdin");
        let stdout = child.stdout.take().expect("take child stdout");
        let stderr: ChildStderr = child.stderr.take().expect("take child stderr");

        // Drain stderr on a background thread so a chatty run (RUST_LOG=trace
        // over ~20 tool calls) can never fill the pipe buffer and deadlock
        // against this process still writing requests to stdin.
        let stderr_buffer = Arc::new(Mutex::new(String::new()));
        let buffer_handle = Arc::clone(&stderr_buffer);
        let stderr_reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => buffer_handle
                        .lock()
                        .expect("stderr buffer lock")
                        .push_str(&line),
                }
            }
        });

        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stdout_lines: Vec::new(),
            stderr_buffer,
            stderr_reader: Some(stderr_reader),
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
        assert!(bytes > 0, "server closed stdout before sending a response");
        let trimmed = line.trim_end().to_string();
        let parsed = serde_json::from_str(&trimmed).unwrap_or_else(|e| {
            panic!("response line was not valid JSON-RPC: {e}\nline: {trimmed:?}")
        });
        self.stdout_lines.push(trimmed);
        parsed
    }

    /// Stop the server and return every raw line it wrote to stdout (already
    /// collected as each response was read during the flow — the pipe has
    /// nothing left in it by the time the server exits) and to stderr while
    /// it ran.
    fn finish(mut self) -> (Vec<String>, String) {
        drop(self.stdin);
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.stderr_reader.take() {
            let _ = handle.join();
        }
        let stderr = self
            .stderr_buffer
            .lock()
            .expect("stderr buffer lock")
            .clone();
        (self.stdout_lines, stderr)
    }
}

impl Transport for SubprocessTransport {
    async fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        self.receive()
    }

    async fn notify(&mut self, method: &str, params: Value) {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }
}

/// The level word `tracing_subscriber`'s default console formatter writes
/// right after the timestamp (`2026-08-07T18:02:59Z DEBUG span: target:
/// message field=value`). "At INFO or louder" means ERROR, WARN or INFO.
fn line_level(line: &str) -> Option<&str> {
    line.split_whitespace().nth(1)
}

fn is_info_or_louder(level: &str) -> bool {
    matches!(level, "ERROR" | "WARN" | "INFO")
}

#[tokio::test]
async fn stdout_is_pure_json_rpc_and_no_sentinel_reaches_an_info_line() {
    let root = TempDir::new().expect("tempdir");
    let mut transport = SubprocessTransport::spawn(root.path());

    let result = support::run_content_leak_flow(&mut transport).await;

    let (stdout_lines, stderr) = transport.finish();

    // ---- coverage: every real tool must have been exercised -----------------
    // Independent of `telemetry_span_fields.rs`'s own coverage check: this
    // file must fail on its own if a tool is added without a matching flow
    // step, not only when run alongside the other content test.
    let expected: std::collections::BTreeSet<String> = tasks_mcp::tools::tool_definitions()
        .into_iter()
        .map(|t| t.name)
        .collect();
    let exercised: std::collections::BTreeSet<String> =
        result.exercised_tools.iter().cloned().collect();
    assert_eq!(
        exercised,
        expected,
        "every tool in tool_definitions() must have a step in \
         support::run_content_leak_flow; missing: {:?}, extra: {:?}",
        expected.difference(&exercised).collect::<Vec<_>>(),
        exercised.difference(&expected).collect::<Vec<_>>()
    );

    // ---- stdout carries only JSON-RPC ----------------------------------------
    assert!(
        !stdout_lines.is_empty(),
        "the flow must have produced at least one response line"
    );
    for line in &stdout_lines {
        assert!(
            serde_json::from_str::<Value>(line).is_ok(),
            "every stdout line must be valid JSON-RPC, even at RUST_LOG=trace: {line:?}"
        );
    }

    // ---- no sentinel reaches an INFO-or-louder stderr line --------------------
    let stderr_lines: Vec<&str> = stderr.lines().collect();
    for sentinel in &result.sentinels {
        for line in &stderr_lines {
            let Some(level) = line_level(line) else {
                continue;
            };
            if is_info_or_louder(level) {
                assert!(
                    !line.contains(sentinel.as_str()),
                    "an {level} console line leaked content: {line}"
                );
            }
        }
    }

    // ---- positive control: the console really carries the content, at DEBUG ---
    for sentinel in &result.sentinels {
        let seen_at_debug_or_trace = stderr_lines.iter().any(|line| {
            matches!(line_level(line), Some("DEBUG") | Some("TRACE"))
                && line.contains(sentinel.as_str())
        });
        assert!(
            seen_at_debug_or_trace,
            "sentinel {sentinel} never appeared on a DEBUG/TRACE console line \
             -- the absence check above would pass even if the console were \
             not actually carrying tool arguments at RUST_LOG=trace"
        );
    }
}
