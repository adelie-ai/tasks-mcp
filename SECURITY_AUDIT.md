# Security Audit — tasks-mcp

**Date:** 2026-03-31
**Scope:** Task management MCP server

---

## High Severity

### 1. Unbounded Memory Allocation from Content-Length

**File:** `src/transport.rs:155`

```rust
let mut body = vec![0_u8; content_length];
```

The Content-Length header value is used directly to allocate a buffer with no upper bound. A malicious client can send a multi-GB Content-Length to exhaust memory.

**Recommendation:** Add a maximum Content-Length check (e.g. 10 MiB) before allocation.

---

### 2. Path Traversal via Task ID

**File:** `src/storage.rs:94-104`

`task_file_path()` constructs paths using the `id` parameter without validation:

```rust
self.type_dir(list, task_type)
    .join(format!("{id} - {slug}.md"))
```

While `id` is typically generated internally, if an attacker can control it (e.g. via `update_task`), path traversal is possible.

**Recommendation:** Validate that `id` contains only alphanumeric characters and hyphens. Canonicalize the final path and verify it remains within the root directory.

---

## Medium Severity

### 3. No Access Control on Task Lists

**File:** `src/operations/task_ops.rs:306-337`

`search_tasks()` searches across all task lists without authorization. Any connected client can read all task data.

**Recommendation:** Acceptable for single-user local use. Document the trust model.

---

### 4. No Pagination on List Operations

**File:** `src/storage.rs:56-77, 154-193`

List operations load all files into memory without limits.

**Recommendation:** Add optional pagination parameters.

---

## Positive Findings

- List names validated for path separators
- Title slugification prevents most path issues
- No shell command execution
- No `unsafe` code
