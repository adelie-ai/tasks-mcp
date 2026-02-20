# Feature: Add a `validating` task status

**Priority:** Medium  
**Area:** task model / workflow

## Background
In real workflows we often have a state between:
- *work completed by external party* (e.g., ServiceNow ticket closed), and
- *task confirmed complete by the user* (validation / access verification).

Today, the task status enum supports only:
- `todo`, `doing`, `blocked`, `done`, `canceled`

This forces users to keep tasks in `doing` (or mark `blocked`) even when the only remaining work is validation.

## Problem
We need a first-class way to represent “pending validation” so that:
- task lists accurately reflect work in progress vs verification steps
- users can filter/report on validation backlog
- the assistant can set status consistently without encoding state only in notes

## Proposal
Add a new task status:
- `validating`

Optionally, consider synonyms/alternatives if you prefer different wording:
- `review`
- `verify`
- `pending_validation`

## Desired behavior
- `set_status(..., status="validating")` is accepted.
- `list_tasks(status="validating")` filters correctly.
- Status is included in task frontmatter/status field and in tool responses.

## Compatibility / migration
- Existing tasks remain valid.
- No change required unless users opt-in.

## Acceptance criteria
- [ ] Status enum includes `validating`.
- [ ] CLI/MCP/tooling accepts and returns `validating`.
- [ ] Tests updated/added to cover setting + listing.
- [ ] Documentation updated to mention the new status.
