# tasks-mcp

`tasks-mcp` is a Rust MCP server for local task storage and task management using Markdown files with YAML frontmatter.

It follows the `tasks-mcp` specification in [docs/spec.md](docs/spec.md):

- XDG-backed storage under `~/.local/share/desktop-assistant/tasks/`
- Multiple lists/contexts as directories
- One-level hierarchy: `epic` -> `deliverable`
- Required frontmatter + flexible markdown body
- Atomic file writes (temp + rename)

## Build

```bash
cargo build --release
```

## Run

STDIO mode (default and recommended for editor integration):

```bash
./target/release/tasks-mcp serve --mode stdio
```

WebSocket mode:

```bash
./target/release/tasks-mcp serve --mode websocket --host 0.0.0.0 --port 8080
```


## Available tools

- `list_lists`
- `create_list`
- `create_task`
- `get_task`
- `update_task`
- `delete_task`
- `list_tasks`
- `search_tasks`
- `add_deliverable`
- `remove_deliverable`

## KDE Widget and MCP Service Bundling

This project includes a KDE widget, which is bundled with the MCP service/server. The philosophy is that MCP-specific UI functionality is distributed together with the MCP service that provides it (at least for now). This approach ensures that users have access to integrated UI features directly from the MCP service, simplifying deployment and usage.

## DBUS API

The MCP server also provides a DBUS API equivalent to the functions exposed to the LLM, allowing desktop UI components to directly manipulate tasks and lists. This enables seamless integration between the server and desktop environments for task management without requiring the LLM to mediate deterministic actions.

## Logging

Traces, metrics and logs come from `mcp-core`, through `adelie-telemetry`. Full
mechanics (subscriber setup, the metrics facade, span-close events, shutdown
timing) are documented once in the [mcp-core
README](https://github.com/adelie-ai/mcp-core#logging); this section covers
what is specific to `tasks-mcp`.

Unlike most servers in the fleet, `tasks-mcp` does not use `mcp_core::run` --
`main.rs` builds `ServerCore` directly and shares that construction path with
an in-process host (da#538), which must never install its own subscriber. So
this binary installs one itself, once, at the top of `main`, and holds the
guard for the life of the process.

### Where it goes

**stderr, always.** The stdio transport frames JSON-RPC on stdout, so a log
line there would corrupt the protocol. This holds at every level, including
`RUST_LOG=trace`.

```bash
RUST_LOG=debug tasks-mcp serve --mode stdio
```

### The level contract, and why it matters more here

| Level | Carries |
|---|---|
| INFO | ids, counts, durations, tool names, D-Bus operation names. **Never content.** |
| DEBUG | tool arguments, and the reason a tool declined or failed. |

A task's title, body, and notes are the content in this server. They never
reach a span field or an INFO line, at any log level. `RUST_LOG=debug` is
what it takes to see the assembled MCP tool arguments (via mcp-core's own
dispatch layer, sanitised and size-capped) -- that is deliberate, not this
server's addition.

The D-Bus surface goes further: it logs no argument content at any level,
including DEBUG. A D-Bus call carries only its operation name and its
ok/error outcome, never the JSON payload a caller sent.

### What this server emits

mcp-core's dispatch layer already covers the JSON-RPC request and the tool
call: `mcp.request` and `mcp.tools.call` spans, and the `mcp.requests` /
`mcp.tools.call` / `mcp.tools.call.duration` metrics, all keyed by method or
tool name, never by argument content.

D-Bus calls bypass that dispatcher entirely, so `tasks-mcp` mirrors it for
that surface:

- A `tasks_mcp.dbus.call` span per D-Bus method, carrying only the operation
  name (`list_tasks`, `create_task`, and so on -- a fixed, compile-time
  vocabulary, never a caller-supplied string).
- A `tasks_mcp.dbus.call` counter, labelled `operation` and `outcome` (`ok` /
  `error`), and a `tasks_mcp.dbus.call.duration` histogram.
- A `tasks_mcp.dbus.startup_failure` counter, labelled by a bounded `reason`
  (`no_session_bus`, `name_taken`, `setup_failed`), if the D-Bus service
  fails to start. An absent session bus is an expected condition on a
  headless box or in a container (see `AGENTS.md`'s capability-based
  degradation rule) and logs at `WARN`; a name conflict or any other setup
  failure is a genuine fault and logs at `ERROR`. Either way the MCP
  transport keeps serving tool calls -- the D-Bus service is a best-effort
  side channel, not a dependency of it.

### Exporting to a collector

Off by default (`otel` feature, see `Cargo.toml`). With it off, no
opentelemetry crate is resolved at all. With it on, configure export with the
standard `OTEL_EXPORTER_OTLP_*` environment variables -- there are no CLI
flags and no server-specific variables. See the [mcp-core
README](https://github.com/adelie-ai/mcp-core#exporting-to-a-collector) for
the full variable reference.

```bash
cargo build --release --features otel
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 \
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf \
  ./target/release/tasks-mcp serve --mode stdio
```

With no collector configured, the periodic metrics summary still writes to
stderr, so a default-feature install from `cargo install` gets real numbers
in the journal.

## Development

Run checks:

```bash
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Testing note

Integration tests in `tests/task_ops.rs` run against a temporary storage root.

## License

Apache-2.0.
