# Phase 3J — Guide Archive & Restore

## Objective

Archive and restore a Guide through a revision-checked recoverable metadata update while preserving its file, content, Category, Attachments, and unknown metadata.

## Required reasoning level

Orchestrator/high; no extra agent for this bounded slice.

## Prerequisites

Signed Category-removal checkpoint `dac22e4`; ADR 0014's exact Guide lifecycle matrix; the accepted recovery update executor and native retained Workspace state.

## Allowed files or subsystem

`crates/rehow-core/src/domain/content/mutation.rs`, core application/tests, desktop Tauri/React/CSS/tests, this packet, and `PROGRESS.md`.

## Relevant specifications and ADRs

Guide status rules in `docs/content-model.md`; ADR 0009; ADR 0014; `docs/threat-model.md`; and `docs/design-direction.md`.

## Exact required behavior

- `Archive` maps draft to archived-from-draft and stale/tested to archived-from-stale; it records the trusted operation timestamp and is a no-op when already archived.
- `Restore` is valid only for archived Guides; archived-from-draft restores draft and archived-from-stale/tested restores stale; it removes both archive metadata fields.
- Every change updates `updated_at`, preserves the Guide path/body/Category/verification pair/Attachments/unknown metadata, uses the expected revision, and commits through existing recovery.
- The native command accepts only bounded Guide ID, revision, and Boolean archive intent against retained Workspace authority.
- Reference Desk exposes named Archive/Restore controls with busy, live, focused error, and predictable focus-restoration behavior.

## Important states and edge cases

Draft, stale, tested, already archived, legacy archived-from-tested, restore on non-archived, stale revision, missing/malformed/read-only Guide, unsafe comments/presentation, contention, and committed-refresh failure.

## Security constraints

No move/delete, generic YAML/status/path, caller time/operation ID, command execution, Git, or MCP write. Tested content is never created or retained as tested by this pre-approval lifecycle transition.

## Tests to add or update

Planner lifecycle matrix and preservation/refusal tests; application revision/path/content/unknown preservation tests; stable native error tests; focused UI helper/static semantics tests where the current non-DOM harness can cover them.

## Commands to run

Focused Cargo/Vitest/type checks, then one full pnpm/Cargo/Clippy/build/Tauri/bootstrap/whitespace gate.

## Required reviews and artifacts

Fresh current web-interface-guideline static review of changed UI. Defer screenshots/browser interaction to the documented combined UI gate.

## Explicit non-goals

Mark tested, Guide deletion, Category move, path tidy, Tags/Aliases, Attachments, Git, MCP mutation, dependencies, or a new browser test harness.

## Definition of done

The exact lifecycle matrix is implemented and revision-safe, all preserved state remains byte/semantically stable, required gates pass, and signed feature/progress checkpoints exist.

## Expected commit boundary

One signed feature commit and one signed progress-only commit; the Orchestrator owns both.

## Conditions requiring escalation

Any need for new storage semantics, generic mutation authority, tested-state approval, content movement/deletion, dependency changes, or unresolved safety ambiguity.
