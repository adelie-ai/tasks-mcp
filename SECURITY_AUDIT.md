# Security Audit — tasks-mcp

**Date:** 2026-03-31
**Scope:** Task management MCP server

---

## High Severity

### 1. Path Traversal via Task ID (HIGH)

**File:** `src/storage.rs:94-104`

`task_file_path()` constructs paths using the `id` parameter without validation. While `id` is typically generated internally, if an attacker can control it, path traversal is possible.

**Recommendation:** Validate that `id` contains only alphanumeric characters and hyphens. Canonicalize the final path and verify it remains within the root directory.

---

## Medium Severity

### 2. No Pagination on List Operations (MEDIUM)

**File:** `src/storage.rs:56-77, 154-193`

List operations load all files into memory without limits.

**Recommendation:** Add optional pagination parameters.

---

## Resolved (2026-03-31)

- Content-Length DoS — 10 MiB limit added to transport

## Positive Findings

- List names validated for path separators
- Title slugification prevents most path issues
- No shell command execution
- No `unsafe` code
