# AGENTS

Project guidance for coding agents and contributors working in this repository.

## Scope and priorities

- Keep changes focused and minimal.
- Fix root causes rather than layering temporary workarounds.
- Avoid unrelated refactors while implementing a requested change.

## Cross-agent operating rules

- Be concise and direct in code and communication.
- Prefer the smallest change that fully solves the requested problem.
- Complete work end-to-end when feasible (implement + validate), not just analysis.
- If requirements are ambiguous, choose the simplest interpretation that matches existing behavior.
- Do not add speculative features, broad rewrites, or unrelated cleanup.
- Do not commit, create branches, or alter repository history unless explicitly requested.

## Rust code style

- Follow existing style and naming patterns in the repository.
- Keep functions explicit and straightforward.
- Avoid one-letter variable names except for tight loop indices.
- Do not add dependencies unless they materially simplify or harden the implementation.

## Testing policy

For behavior changes:

1. Add or adjust tests covering expected behavior.
2. Implement code changes.
3. Re-run targeted tests first, then broader suites.

Before finishing:

- Run `cargo test`.
- Run `cargo clippy --all-targets --all-features -- -D warnings`.
- Keep the project warning-free under `#![deny(warnings)]`.

## MCP-specific notes

- Keep initialization gating semantics for MCP methods (`initialize` then `initialized`).
- Keep tool names and argument contracts stable unless explicitly changing the spec.
- Return structured, machine-consumable tool outputs.
