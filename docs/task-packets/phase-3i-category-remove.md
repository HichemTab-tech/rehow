# Phase 3I — Safe empty Category removal

## Objective

Remove an empty, non-default Category through recoverable trash after explicit user confirmation, without any cascade behavior.

## Required reasoning level

Architect/xhigh for recoverable directory cleanup and Orchestrator/high for adapter/UI integration.

## Prerequisites

- Accepted ADR 0009 trash state machine and descriptor confinement.
- Category lifecycle/revision payloads and native retained Workspace authority.

## Allowed files or subsystem

- ADR 0009 and this packet
- Core storage recovery/application Category paths
- Desktop Tauri/React/CSS and focused tests
- `PROGRESS.md`

## Relevant specifications and ADRs

- `docs/content-model.md`
- `docs/adr/0009-recovery-journal-and-backup-state-machine.md`
- `docs/threat-model.md`

## Exact required behavior

- Require Category ID and current descriptor revision.
- Refuse the manifest default Category, any Category containing a Guide, any malformed/extra directory entry, stale revisions, or unsafe storage.
- Journal-move `category.yaml` to the exact operation trash path.
- Remove the now-empty live Category directory before the delete journal commits; replay this cleanup idempotently after interruption.
- Preserve the trash copy and every unrelated Category/Guide byte.
- Refresh the Workspace only after durable success.
- Require a native UI confirmation naming the Category; show busy and actionable errors.

## Important states and edge cases

- Default/nonempty/archived/last Category; unexpected files; stale revision; missing/replaced directory; interruption after descriptor move and after directory removal.

## Security constraints

- Never recursively remove a directory and never delete Guides or unknown files.
- Validate the exact directory inventory under the writer lock.
- Use `unlinkat(AT_REMOVEDIR)` only for the validated Category component and sync its parent.
- Preserve conflicting or ambiguous state and report recovery/conflict.

## Tests to add or update

- Empty removal and trash preservation; default/nonempty/extra-entry/stale refusal; stable unrelated content; idempotent empty-directory cleanup through existing replay tests where applicable; native errors and UI ordering helper coverage.

## Commands to run

- Focused Cargo/Vitest while editing, then one combined root check/test/build/Clippy/Tauri gate.

## Explicit non-goals

- Cascade delete, force removal, default replacement, permanent trash purge, restore UI, Guide deletion, MCP mutation, or arbitrary paths.

## Definition of done

- Required behavior and tests pass, current UI guidelines pass, docs reflect reality, and signed checkpoints exist.

## Expected commit boundary

One feature commit plus one progress-only commit.

## Conditions requiring escalation

- Safe idempotent directory cleanup cannot be expressed without changing the journal wire schema or any removal would require recursion.
