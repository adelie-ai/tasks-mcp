# Requested enhancements

This document captures enhancements that would make the Tasks MCP safer/easier to use from an assistant (and for humans editing task markdown directly).

## Background / incident

We hit a failure when trying to mark a task as `doing` after adding an external ticket number.

- A note was inserted into the **YAML frontmatter block** of a task markdown file.
- The inserted note contained Markdown bold (e.g. `**TICKET-123**`).
- YAML parsing subsequently failed with an error similar to:
  - `while scanning an alias ...` (YAML treats `*name` as an alias; `**...**` can trigger this depending on the parser/context)
- Once the frontmatter is invalid, normal task operations (e.g. `set_status`) can fail because they cannot parse the file.

This happened because a general-purpose file edit (line-based insertion) was used instead of task-aware tooling.

## Goals

1. Make it hard/impossible to corrupt task frontmatter when adding notes.
2. Provide task-native APIs for common updates (ticket numbers, notes, links) so callers do not need to edit raw markdown.
3. If corruption does occur, provide recovery/repair tooling.

## Enhancements

### 1) Append a note to a task body (task-native)

**Need:** A task tool that appends a note to the *body* (after frontmatter), without the caller needing to parse/modify markdown.

**Proposed API:**

- `append_task_note({ id, note, section? })`

**Behavior:**
- Reads the task file.
- Ensures YAML frontmatter remains unchanged/valid.
- Appends `note` to the end of the body, or under a named section (e.g. `## Notes`).
- Optionally prefixes the note with a timestamp.
- Updates the task `updated:` timestamp.

**Why:** Ticket numbers, access confirmations, and short updates are extremely common and should not require file editing.

Example usage:
- Append: `Ticket: TICKET-123`
- Append: `Note: some freeform update text`

### 2) Structured metadata fields for external ticket references

**Need:** A first-class way to store external ticket references without mixing them into freeform body text.

**Proposed API:**

- `add_external_ref({ id, system, ref, url? })`

**Task file representation options:**

Option A (frontmatter list):
```yaml
external_refs:
  - system: jira
    ref: PROJ-123
    url: https://... (optional)
```

Option B (single string map for convenience):
```yaml
external_refs:
  jira: PROJ-123
```

**Behavior:**
- Validates allowed systems (or accepts arbitrary strings).
- Deduplicates.
- Keeps frontmatter YAML valid (quotes values if needed).
- Updates `updated:`.

**Why:** Enables consistent querying/filtering/reporting (e.g. "show all tasks with Jira tickets").

### 3) Update a task body safely (task-native patch/append)

**Need:** A higher-level `update_task` that can edit body without callers doing file edits.

**Proposed API additions to existing `update_task`:**

- `update_task({ id, body_append?, body_prepend?, body_replace?, body_patch? })`

Where `body_patch` could be a limited set of safe operations:
- insert after heading
- insert before heading
- replace a literal substring

**Behavior:**
- Task tooling should locate frontmatter end delimiter (`---`) and only operate on body content.
- If the file has invalid frontmatter, fail with a clear error (see repair tool below).

### 4) Repair tool for invalid frontmatter

**Need:** If a task file becomes invalid YAML, there should be a supported way to recover.

**Proposed API:**

- `repair_task_frontmatter({ id, strategy })`

**Possible strategies:**
- `strategy: "salvage"` — try to parse what can be parsed; move unknown/invalid lines into body under a `## Recovered` section.
- `strategy: "reset"` — rewrite frontmatter based on minimal required fields + known fields from filename/id index.

**Behavior:**
- Produces a diff or a before/after preview.
- Never deletes content silently.

### 5) Guardrails: prevent non-task tools from writing into frontmatter (optional)

This may be out of scope for the Tasks MCP itself, but two ideas:

- Provide an exported constant/utility (e.g. `FRONTMATTER_BOUNDARY`) and helper functions to detect it.
- Add a “safe edit” endpoint that only edits the body region.

## Acceptance criteria

- A caller can add an external ticket number and then call `set_status` without any risk of YAML corruption.
- Tasks with external ticket numbers can be queried programmatically.
- If a file is corrupted, there is a documented/implemented recovery path.

## Notes

The underlying issue is not that Markdown is bad; it’s that YAML frontmatter is sensitive, and many common note formats (especially those using `*` or `:`) need quoting/escaping.
A task-native note/ticket API would remove this entire class of errors.
