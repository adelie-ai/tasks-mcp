//! In-process capturing-layer test for mcp-core's dispatch path
//! (adelie-ai/mcp-core#40).
//!
//! A console-text test cannot see a span-field leak: a span records its
//! fields at creation, and nothing prints them unless an event fires inside
//! the span or span-close events are enabled -- and no MCP server enables
//! those. So a captured argument can sit in a span, invisible on the
//! console, and still export over OTLP when the span closes. This test reads
//! the spans back directly instead, with a capturing `tracing` layer.
//!
//! Lesson 8: the flow in `support::run_content_leak_flow` exercises every
//! tool `tool_definitions()` advertises, not just one. A leak-detection test
//! scoped to a single tool proves the mechanism works for that tool and says
//! nothing about the rest; the coverage assertion below fails if a tool is
//! ever added to the server without a matching step in that flow.

#![deny(warnings)]

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use mcp_core::{ServerCore, Session};
use serde_json::Value;
use support::Transport;
use tasks_mcp::service::TasksService;
use tasks_mcp::storage::Storage;
use tempfile::TempDir;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

// ---- a small capturing layer, local to this file (see src/dbus.rs's own
// test module for why each caller keeps its own copy rather than sharing one).

#[derive(Clone, Debug)]
struct RecordedSpan {
    name: &'static str,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
struct RecordedEvent {
    level: tracing::Level,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
struct Capture(
    Arc<Mutex<Vec<RecordedSpan>>>,
    Arc<Mutex<Vec<RecordedEvent>>>,
);

impl<S> Layer<S> for Capture
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {
        let mut fields = BTreeMap::new();
        attrs.record(&mut Collector(&mut fields));
        self.0.lock().expect("capture lock").push(RecordedSpan {
            name: attrs.metadata().name(),
            fields,
        });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let name = ctx.span(id).map_or("<closed>", |span| span.name());
        let mut fields = BTreeMap::new();
        values.record(&mut Collector(&mut fields));
        self.0
            .lock()
            .expect("capture lock")
            .push(RecordedSpan { name, fields });
    }

    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = BTreeMap::new();
        event.record(&mut Collector(&mut fields));
        self.1.lock().expect("capture lock").push(RecordedEvent {
            level: *event.metadata().level(),
            fields,
        });
    }
}

struct Collector<'a>(&'a mut BTreeMap<String, String>);

impl Visit for Collector<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

struct InProcessTransport {
    session: Session,
}

impl Transport for InProcessTransport {
    async fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        let dispatch = self
            .session
            .handle_message(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            }))
            .await;
        dispatch.response.unwrap_or(Value::Null)
    }

    async fn notify(&mut self, method: &str, params: Value) {
        let _ = self
            .session
            .handle_message(serde_json::json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
            }))
            .await;
    }
}

/// Acceptance: every tool this server advertises can be called end to end
/// with a sentinel in its content-bearing argument, and none of those
/// sentinels ever reach a `mcp.tools.call` span field or an INFO-or-louder
/// event. mcp-core's own dispatch already guarantees this generically; this
/// test is tasks-mcp's own regression tripwire, table-driven over the real
/// tool list rather than one hand-picked tool.
#[test]
fn every_tool_is_exercised_without_leaking_content() {
    let dir = TempDir::new().expect("tempdir");
    let storage = Storage::with_root(dir.path());
    let service = TasksService::new(storage);
    let core = ServerCore::new(tasks_mcp::server_config(), Arc::new(service));

    let capture = Capture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());
    let result = tracing::subscriber::with_default(subscriber, || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime");
        runtime.block_on(async {
            let mut transport = InProcessTransport {
                session: Session::new(core),
            };
            support::run_content_leak_flow(&mut transport).await
        })
    });

    let spans = capture.0.lock().expect("capture lock").clone();
    let events = capture.1.lock().expect("capture lock").clone();

    // ---- coverage: every real tool must have been exercised -----------------
    let expected: BTreeSet<String> = tasks_mcp::tools::tool_definitions()
        .into_iter()
        .map(|t| t.name)
        .collect();
    let exercised: BTreeSet<String> = result.exercised_tools.into_iter().collect();
    assert_eq!(
        exercised,
        expected,
        "every tool in tool_definitions() must have a step in \
         support::run_content_leak_flow; missing: {:?}, extra: {:?}",
        expected.difference(&exercised).collect::<Vec<_>>(),
        exercised.difference(&expected).collect::<Vec<_>>()
    );
    assert!(
        !result.sentinels.is_empty(),
        "the flow must plant at least one sentinel"
    );

    // ---- no sentinel ever reaches a span field -------------------------------
    for sentinel in &result.sentinels {
        for span in &spans {
            for (field, value) in &span.fields {
                assert!(
                    !value.contains(sentinel.as_str()),
                    "span {:?} field {field:?} leaked content: {value}",
                    span.name
                );
            }
        }
    }

    // ---- no sentinel ever reaches an INFO-or-louder event --------------------
    for sentinel in &result.sentinels {
        for event in &events {
            if event.level <= tracing::Level::INFO {
                for (field, value) in &event.fields {
                    assert!(
                        !value.contains(sentinel.as_str()),
                        "a {:?} event field {field:?} leaked content: {value}",
                        event.level
                    );
                }
            }
        }
    }

    // ---- positive control: capture really saw the content, at DEBUG ---------
    // Proves the two checks above are checking something, not capturing
    // nothing: mcp-core's own dispatch logs the full tool arguments at DEBUG
    // (never INFO), so every sentinel this flow planted must show up there.
    for sentinel in &result.sentinels {
        let seen_at_debug = events.iter().any(|event| {
            event.level > tracing::Level::INFO
                && event
                    .fields
                    .values()
                    .any(|value| value.contains(sentinel.as_str()))
        });
        assert!(
            seen_at_debug,
            "sentinel {sentinel} was never captured at DEBUG -- the absence \
             checks above would pass even if capture were blind"
        );
    }
}
