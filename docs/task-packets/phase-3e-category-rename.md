# Phase 3E — Category Rename

## Objective

Rename a Category through a revision-checked descriptor update without moving its directory.

## Required reasoning level

Orchestrator/high; no extra agent for this bounded slice.

## Prerequisites

Signed Guide-details checkpoint `66af3d0`; accepted Category mutation planner and recovery executor.

## Allowed files or subsystem

Core application tests, desktop Tauri/React/CSS/tests, this packet, and `PROGRESS.md`.

## Relevant specifications and ADRs

Category/path/mutation sections of `docs/content-model.md`, ADR 0009, ADR 0014, threat model, and Reference Desk design direction.

## Exact required behavior

- Accept Category ID, expected descriptor revision, and name; apply only typed `SetName` under the writer lock.
- Preserve summary, order, status, unknown metadata, Guides, and the existing Category directory.
- A no-op preserves bytes/revision; a real rename refreshes Workspace/search presentation.
- Provide a compact labeled inline form with focused errors and discard protection.

## Important states and edge cases

Missing Category, invalid name, no-op, stale/external change, malformed/read-only descriptor, unsafe round trip, contention, interruption, long text, and narrow rail.

## Security constraints

No renderer path/YAML/time/operation authority, no directory rename, and no overwrite of stale revisions. MCP stays read-only.

## Tests to add or update

Core rename/path/no-op/invalid/stale tests; native revision payload/error tests; UI normalization and form semantics.

## Commands to run

Focused Cargo/Vitest/type checks, then one full pnpm/Cargo/Clippy/build/Tauri/bootstrap/whitespace gate.

## Explicit non-goals

Summary editing, reorder, archive/restore/remove, path tidy, Guide moves, Git, MCP mutation, or dependencies.

## Definition of done

Category name refreshes everywhere without moving content; applicable gates pass; signed checkpoint and restart state exist.

## Expected commit boundary

One signed feature commit and one signed progress-only commit.

## Conditions requiring escalation

Any need for generic YAML/path mutation, directory movement, destructive behavior, dependency addition, or new storage semantics.
