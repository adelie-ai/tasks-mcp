#![deny(warnings)]

// MF-12 (sibling of terminal-mcp): tasks-mcp must refuse the websocket
// transport. mcp-core's websocket transport is unauthenticated, so
// `serve --transport websocket --host 0.0.0.0` would expose every task —
// reads and writes — to anyone who can reach the port. The server is
// stdio-served (plus D-Bus) in practice.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Spawn `tasks-mcp serve --transport websocket` and require it to exit
/// promptly with a failure instead of binding a listener.
#[test]
fn serve_websocket_is_refused() {
    let tasks_root = TempDir::new().expect("create temp tasks root");

    let mut child = Command::new(env!("CARGO_BIN_EXE_tasks-mcp"))
        // --no-dbus keeps the test hermetic (no session-bus dependency);
        // --port 0 avoids a real bind if a regression lets it get that far.
        .args([
            "serve",
            "--transport",
            "websocket",
            "--port",
            "0",
            "--no-dbus",
        ])
        .env("TASKS_MCP_ROOT", tasks_root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tasks-mcp serve --transport websocket");

    // The refusal happens before any I/O, so the process should exit almost
    // immediately. Poll with a generous deadline; if it is still running it
    // accepted the transport (the bug), so kill it and fail.
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait().expect("try_wait on child") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                child.kill().expect("kill lingering websocket server");
                child.wait().expect("reap killed child");
                panic!("tasks-mcp served the websocket transport instead of refusing it");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };

    assert!(
        !status.success(),
        "serve --transport websocket must exit with a failure status"
    );

    let mut stderr = String::new();
    use std::io::Read;
    child
        .stderr
        .take()
        .expect("child stderr")
        .read_to_string(&mut stderr)
        .expect("read child stderr");
    assert!(
        stderr.to_lowercase().contains("websocket"),
        "refusal error should name the websocket transport; stderr was: {stderr}"
    );
}
