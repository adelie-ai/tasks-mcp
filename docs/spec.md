# tasks-mcp — Specification

## 1. Purpose
`tasks-mcp` is a small task-storage and task-management service designed to be used by an LLM/desktop-assistant via MCP-style file operations.

Goals:
- Store tasks as human-readable **Markdown files**.
- Organize tasks into multiple **lists/contexts** using directories (e.g. `work-project-1/`, `personal-project-2/`) where "work" and "personal" are not significant in this context, just part of the name.
- Support a single level of hierarchy by differentiating **Epic tasks** (parent) vs **Deliverable tasks** (leaf).
- Be easy to inspect and edit manually (in an editor) and robust for programmatic updates.

Non-goals:
- Multi-level subtask trees (beyond epics → deliverables).
- Complex scheduling, dependencies, or time tracking.
- Replacing Jira; this is a personal/local task system.

## 2. Storage location (XDG)
All tasks are stored under the same XDG base directory as the desktop assistant (determine it the same way):

- XDG base: `~/.local/share/desktop-assistant/`
- Tasks root: `~/.local/share/desktop-assistant/tasks/`

Within `tasks/`, each **list/context** is a directory:

- `~/.local/share/desktop-assistant/tasks/work-project-1/`
- `~/.local/share/desktop-assistant/tasks/personal-project-2/`

This directory separation is the only required mechanism for “multiple lists”.

## 3. Task model
A task is represented by a single markdown file.

### 3.1 Task types
Two task types exist:

1) **Epic**
- A parent/umbrella task intended to be broken into smaller deliverables.
- Contains references to deliverables.

2) **Deliverable**
- A smaller unit of work that can be completed.
- Optionally references its parent epic.

Only one layer is supported:
- Epic → Deliverables
- Deliverables do not have children.

### 3.2 Task identity
Each task file name begins with an **ID** to provide stable references even if the title changes.

Recommended ID format:
- `tsk-YYYYMMDD-HHMMSS` (local timestamp), OR
- `tsk-<random>` (short unique string)

File name format:
- `<id> - <slug>.md`

Examples:
- `tsk-20260218-154455 - project-alpha-onboarding.md`
- `tsk-20260218-154612 - request-environment-access.md`

## 4. Markdown format
Each task file is markdown with a required frontmatter block.

### 4.1 Frontmatter (required)
YAML frontmatter fields:

- `id` (string, required): stable ID
- `title` (string, required)
- `type` (enum, required): `epic` | `deliverable`
- `status` (enum, required): `todo` | `doing` | `blocked` | `done` | `canceled`
- `list` (string, required): list/context directory name (e.g. `project-alpha`)
- `created` (ISO-8601 datetime, required)
- `updated` (ISO-8601 datetime, required)

Optional fields:
- `epic_id` (string): for deliverables, the parent epic id
- `deliverable_ids` (string[]): for epics, list of child deliverable ids
- `tags` (string[]): freeform tags
- `priority` (enum): `p0` | `p1` | `p2` | `p3`
- `due` (ISO-8601 date): due date
- `links` (string[]): URLs, tickets, docs
- `assignee` (string): usually omitted for personal use; default implicit = current user

### 4.2 Body (recommended sections)
Recommended markdown structure:

- Summary (1–3 lines)
- Checklist (optional)
- Notes / log (optional)

The body is intentionally flexible.

## 5. Directory structure
```
~/.local/share/desktop-assistant/tasks/
  <list>/
    epics/
      <id> - <slug>.md
    deliverables/
      <id> - <slug>.md
```

Rationale:
- Makes it trivial to browse epics vs deliverables.
- Still keeps “multiple lists” separated by `<list>`.

Alternative (allowed but not preferred): a flat directory per list. The tool should support both, but it should **create** the preferred structure.

## 6. Core operations (API surface)
The following operations are expected.

### 6.1 List management
- `list_lists()` → returns available list/context names (directories under tasks root)
- `create_list(name)` → creates `<name>/epics` and `<name>/deliverables`

### 6.2 Task CRUD
- `create_task(list, type, title, ...)` → writes a new markdown file, returns `{id, path}`
- `get_task(id | path)` → returns parsed frontmatter + body
- `update_task(id | path, patch)` → updates frontmatter/body and touches `updated`
- `delete_task(id | path)` → removes the file

### 6.3 Query
- `list_tasks(list, type?, status?, tag?, epic_id?)` → returns summaries
- `search_tasks(text, list?)` → grep-like search of body and title

### 6.4 Epic relationships
- `add_deliverable(epic_id, deliverable_id)`
  - updates epic `deliverable_ids`
  - updates deliverable `epic_id`
- `remove_deliverable(epic_id, deliverable_id)`

Invariants:
- A deliverable can have **0 or 1** epic.
- An epic can have **0..N** deliverables.

## 7. Status semantics
- `todo`: not started
- `doing`: actively being worked
- `blocked`: waiting on something external
- `done`: completed
- `canceled`: intentionally dropped

Recommended automation:
- When any field changes, update `updated` timestamp.

## 8. Example tasks

### 8.1 Epic example
Path:
`~/.local/share/desktop-assistant/tasks/project-alpha/epics/tsk-20260218-154455 - platform-onboarding.md`

```markdown
---
id: tsk-20260218-154455
title: Platform onboarding
type: epic
status: doing
list: project-alpha
created: 2026-02-18T15:44:55-05:00
updated: 2026-02-18T16:02:10-05:00
deliverable_ids:
  - tsk-20260218-154612
  - tsk-20260218-154640
tags: [setup, access]
links:
  - https://docs.example.com/project/overview
---

Summary: Complete setup tasks needed to contribute to the project.

## Notes
- Track environment access limitations and blockers.
```

### 8.2 Deliverable example
Path:
`~/.local/share/desktop-assistant/tasks/project-alpha/deliverables/tsk-20260218-154612 - request-environment-access.md`

```markdown
---
id: tsk-20260218-154612
title: Request environment access for dev/services
type: deliverable
status: todo
list: project-alpha
created: 2026-02-18T15:46:12-05:00
updated: 2026-02-18T15:46:12-05:00
epic_id: tsk-20260218-154455
tags: [access, environment]
---

## Checklist
- [ ] Submit access request
- [ ] Verify permissions are visible in the identity dashboard
- [ ] Validate local authentication tooling works
```

## 9. Implementation notes
- Parsing: use a YAML frontmatter parser; treat missing/invalid frontmatter as an error.
- Slug: generate from title (lowercase, replace spaces with `-`, drop non-alphanumerics).
- File writes: should be atomic (write temp + rename) to avoid partial edits.
- Concurrency: last write wins; optional file lock may be added later.

## 10. Migration / compatibility
There is existing prior convention of storing “tickets” under:
`~/.local/share/desktop-assistant/ticket/<context>/`.

`tasks-mcp` is the forward-looking scheme. A future migration tool may:
- ingest old `ticket/` markdown
- map directories to lists
- infer epic vs deliverable if possible

