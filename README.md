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
