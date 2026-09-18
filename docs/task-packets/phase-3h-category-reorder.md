# Phase 3H — Category reordering

## Objective

Reorder all Categories through one bounded, revision-checked, recoverable multi-document operation and expose keyboard-accessible Move Up/Move Down controls.

## Required reasoning level

Architect/xhigh for the multi-target storage boundary and Orchestrator/high for adapter/UI integration.

## Prerequisites

- Accepted recovery journal and Category mutation checkpoints.
- Typed `CategoryMutation::SetOrder(i64)`.

## Allowed files or subsystem

- `crates/rehow-core/src/storage.rs` and `storage/recovery.rs`
- `crates/rehow-core/src/application.rs`
- Desktop Tauri/React/CSS and focused tests
- This packet and `PROGRESS.md`

## Relevant specifications and ADRs

- `docs/content-model.md` revision/effect-set rules
- `docs/adr/0009-*` recovery journal
- `docs/adr/0014-*` typed Category mutation authority
- `docs/threat-model.md`

## Exact required behavior

- Accept the complete ordered Category ID/revision set for the currently opened Workspace.
- Reject missing, duplicate, unknown, or stale entries before mutation.
- Assign deterministic spaced order values (`1000`, `2000`, …) using checked arithmetic.
- Prepare every candidate from current descriptor bytes under one writer lock.
- Journal all changed descriptors as one `MultiFile` operation; valid no-op order preserves all bytes.
- Preserve IDs, names, summaries, statuses, paths, Guides, and unknown metadata.
- Refresh only after successful completion and return the refreshed Workspace.
- Provide disabled-at-boundary Move Up/Move Down buttons with busy/error/live-region behavior.

## Important states and edge cases

- Zero/one Category, archived Categories, duplicate order values, no-op order, stale revision, external change, unsafe descriptor, interruption, and resource limits.

## Security constraints

- No renderer path/order-number/YAML authority; Rust derives order numbers.
- Exact complete effect set and revisions are mandatory.
- Existing descriptor confinement, no-follow handling, recovery store, and writer lock remain authoritative.

## Tests to add or update

- Multi-document no-op/change/stale/effect-set behavior.
- Category reorder success and preservation through application refresh.
- Stable native error/payload behavior and UI helper ordering.

## Commands to run

- Focused Cargo/Vitest while editing, then one combined root check/test/build/Clippy/Tauri gate.

## Explicit non-goals

- Drag-and-drop, arbitrary numeric order input, Category removal, Guide moves, MCP mutation, Git, or new dependencies.

## Definition of done

- Exact behavior passes tests, documentation reflects the verified checkpoint, and commits are signed.

## Expected commit boundary

One feature commit plus one progress-only commit.

## Conditions requiring escalation

- Recovery journal cannot safely represent the multi-target update or exact effect-set validation cannot be maintained.
