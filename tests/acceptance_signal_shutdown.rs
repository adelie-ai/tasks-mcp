#![deny(warnings)]

//! Acceptance criteria for what tasks-mcp does when a signal stops it
//! (adelie-ai/tasks-mcp#20).
//!
//! tasks-mcp's `build_service()` construction path is shared with an
//! in-process host (da#538), so `main` drives `mcp_core::serve` directly
//! rather than `mcp_core::run`. `run` gained signal handling in
//! adelie-ai/mcp-core#46 and eleven of the thirteen MCP servers inherited it
//! for free; tasks-mcp did not, because it never reaches `run`.
//!
//! Every test here drives the real binary, spawns it, signals it, and reads
//! its real stderr. An in-process test can prove what the telemetry guard
//! does when it is dropped. Only a real process, stopped by the operating
//! system, proves it got as far as dropping it -- which is the whole
//! question this ticket asks.
//!
//! `--no-dbus` keeps every probe hermetic (no session-bus dependency), and
//! each probe gets its own `TASKS_MCP_ROOT` temp directory so parallel test
//! runs never share storage state.
//!
//! Only the stdio transport is covered: tasks-mcp's own `server_config()`
//! refuses the websocket transport (`without_websocket()`, MF-12) and never
//! opts into the unix-socket transport, so stdio is the only transport this
//! server really serves.
//!
//! Each test is named after the criterion it holds, so a failing run names
//! the unmet requirement rather than a line number.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// How long a signalled server may take to stop before the test gives up.
/// The flush itself is bounded by the telemetry guard's own shutdown budget,
/// five seconds by default, so anything past this is a hang rather than a
/// slow collector.
const STOP_TIMEOUT: Duration = Duration::from_secs(20);

/// The line the telemetry guard writes when it closes the last window. Its
/// absence is exactly the loss adelie-ai/tasks-mcp#20 is about.
const SUMMARY: &str = "metrics summary";

/// AC: a server stopped by `SIGTERM` over stdio flushes its telemetry.
#[test]
fn sigterm_over_stdio_flushes_the_final_metrics_summary() {
    let stderr = stdio_probe_stopped_by("TERM").stderr;
    assert!(
        stderr.contains(SUMMARY),
        "a SIGTERM over stdio must still write the final metrics summary, \
         but stderr was: {stderr:?}"
    );
}

/// AC: `SIGINT` behaves the same way as `SIGTERM`.
#[test]
fn sigint_over_stdio_flushes_the_final_metrics_summary() {
    let stderr = stdio_probe_stopped_by("INT").stderr;
    assert!(
        stderr.contains(SUMMARY),
        "a SIGINT must be treated exactly as a SIGTERM, but stderr was: {stderr:?}"
    );
}

/// AC: the flushed summary carries the numbers the run really recorded, not
/// an empty shell of a summary.
#[test]
fn the_flushed_summary_carries_the_counters_the_run_recorded() {
    let stderr = stdio_probe_stopped_by("TERM").stderr;
    assert!(
        stderr.contains("mcp.requests"),
        "the flushed summary must carry the request counter the run recorded, \
         but stderr was: {stderr:?}"
    );
}

/// AC: a signalled server reports a clean exit status rather than dying by
/// the signal. `code()` is `None` for a process killed by a signal, so this
/// distinguishes "handled the signal and returned" from "was killed by it".
#[test]
fn a_signalled_server_exits_zero_rather_than_dying_by_signal() {
    let status = stdio_probe_stopped_by("TERM").status;
    assert_eq!(
        status.code(),
        Some(0),
        "a signalled server must exit 0, not die by the signal; status was {status:?}"
    );
}

/// AC: the stop path writes nothing to stdout. The stdio transport frames
/// JSON-RPC there, and one stray line from a signal handler corrupts the
/// stream for a client that is still reading it.
#[test]
fn the_stop_path_writes_nothing_to_stdout() {
    // `request` already read the one reply the server owed, so everything
    // here is what the stop path added. It has to be nothing at all: a
    // stray line that happened to parse as JSON would corrupt the stream
    // just as surely as one that did not.
    let stopped = stdio_probe_stopped_by("TERM");
    assert!(
        stopped.stdout.is_empty(),
        "the stop path must write nothing to stdout, but it wrote: {:?}",
        stopped.stdout
    );
}

/// AC: two signals in quick succession neither panic nor flush twice.
///
/// Whether the second signal lands during the flush or just after it depends
/// on how long the flush takes, and with no collector configured that is a
/// fraction of a millisecond. This test does not control which of the two
/// happens, so it asserts what must hold either way: the process still exits
/// 0, nothing panics, and exactly one summary is written.
#[test]
fn a_second_signal_during_shutdown_neither_panics_nor_double_flushes() {
    let root = TempDir::new().expect("create temp tasks root");
    let mut probe = Probe::start(root.path());
    probe.request(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);

    probe.signal("TERM");
    // The second signal is best effort: the process may already have gone,
    // and `kill` then reports no such process. The point is that a second
    // one is survivable, not that it is delivered.
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(probe.pid().to_string())
        .status();

    let stopped = probe.finish();
    assert_eq!(
        stopped.status.code(),
        Some(0),
        "a second signal must not stop the process exiting cleanly; status was {:?}. \
         stderr was: {:?}",
        stopped.status,
        stopped.stderr
    );
    assert!(
        !stopped.stderr.contains("panicked"),
        "a second signal must not panic: {:?}",
        stopped.stderr
    );
    let summaries = stopped.stderr.matches(SUMMARY).count();
    assert_eq!(
        summaries, 1,
        "the guard must flush exactly once however many signals arrive, but stderr \
         held {summaries} summaries: {:?}",
        stopped.stderr
    );
}

/// The flush that already worked has to keep working: a client that closes
/// the stdio stream still gets the final summary, and the process still
/// exits 0.
#[test]
fn a_clean_eof_still_flushes_the_final_metrics_summary() {
    let root = TempDir::new().expect("create temp tasks root");
    let mut probe = Probe::start(root.path());
    probe.request(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
    drop(probe.child.stdin.take());

    let stopped = probe.finish();
    assert_eq!(
        stopped.status.code(),
        Some(0),
        "a clean EOF must still exit 0; status was {:?}",
        stopped.status
    );
    assert!(
        stopped.stderr.contains(SUMMARY),
        "a clean EOF must still write the final metrics summary, but stderr was: {:?}",
        stopped.stderr
    );
}

/// Start a stdio probe, drive one request through it so the metrics registry
/// has something in it, then stop it with `signal_name`.
fn stdio_probe_stopped_by(signal_name: &str) -> Stopped {
    let root = TempDir::new().expect("create temp tasks root");
    let mut probe = Probe::start(root.path());
    probe.request(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
    probe.signal(signal_name);
    probe.finish()
}

/// A running probe process, with its stderr being drained on another thread.
struct Probe {
    child: Child,
    stdout: BufReader<std::process::ChildStdout>,
    stderr: StderrTail,
}

/// What a stopped probe left behind.
struct Stopped {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Probe {
    fn start(tasks_root: &std::path::Path) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tasks-mcp"));
        cmd.args(["serve", "--transport", "stdio", "--no-dbus"])
            .env("TASKS_MCP_ROOT", tasks_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("the probe must start");
        let stdout = BufReader::new(child.stdout.take().expect("the probe has a piped stdout"));
        let stderr =
            StderrTail::attach(child.stderr.take().expect("the probe has a piped stderr"));
        Self {
            child,
            stdout,
            stderr,
        }
    }

    /// Send one JSON-RPC request and read its reply, so the caller knows the
    /// server is up and has recorded a metric.
    fn request(&mut self, request: &str) {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .expect("the probe has a piped stdin");
        writeln!(stdin, "{request}").expect("the probe must accept its input");
        stdin.flush().expect("the request must reach the probe");
        let mut reply = String::new();
        self.stdout
            .read_line(&mut reply)
            .expect("the probe must answer the request");
        assert!(
            reply.contains("\"result\""),
            "the probe must be serving before it is signalled, but replied {reply:?}"
        );
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Send `signal_name` (as `kill` names it, so `TERM` or `INT`).
    fn signal(&self, signal_name: &str) {
        let status = Command::new("kill")
            .arg(format!("-{signal_name}"))
            .arg(self.pid().to_string())
            .status()
            .expect("kill must run, or this test proves nothing");
        assert!(
            status.success(),
            "kill -{signal_name} on the probe failed, so no signal was delivered"
        );
    }

    /// Wait for the probe to stop, then collect everything it wrote.
    fn finish(mut self) -> Stopped {
        let status = wait_for_exit(&mut self.child);
        let mut stdout = String::new();
        // The child has exited, so this reads to EOF without blocking.
        self.stdout.read_to_string(&mut stdout).unwrap_or_default();
        Stopped {
            status,
            stdout,
            stderr: self.stderr.finish(),
        }
    }
}

/// Wait for `child` to exit, and fail the test rather than hang if it does
/// not.
fn wait_for_exit(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        match child
            .try_wait()
            .expect("the probe's state must be readable")
        {
            Some(status) => return status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                panic!(
                    "the probe did not stop within {STOP_TIMEOUT:?}, so the shutdown is \
                     not bounded"
                );
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Drains a child's stderr on its own thread.
///
/// Reading it only at the end would deadlock a server that fills the pipe
/// while the test is waiting on something else.
struct StderrTail {
    text: Arc<Mutex<String>>,
    reader: Option<JoinHandle<()>>,
}

impl StderrTail {
    fn attach(stderr: ChildStderr) -> Self {
        let text = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&text);
        let reader = std::thread::spawn(move || {
            let mut lines = BufReader::new(stderr).lines();
            while let Some(Ok(line)) = lines.next() {
                let mut sink = sink.lock().unwrap_or_else(|e| e.into_inner());
                sink.push_str(&line);
                sink.push('\n');
            }
        });
        Self {
            text,
            reader: Some(reader),
        }
    }

    /// Wait for stderr to reach EOF, then return everything it carried.
    fn finish(mut self) -> String {
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.text.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
