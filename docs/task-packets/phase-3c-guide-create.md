# Phase 3C — Draft Guide Creation

## Objective

Create one draft Guide in an existing active Category and open it in the native editor.

## Required reasoning level

Orchestrator/high; no additional agent for this bounded slice.

## Prerequisites

Signed Category creation checkpoint `98e2501`; canonical Guide emitter; ADR 0009 recovery executor; retained native Workspace capability.

## Allowed files or subsystem

`crates/rehow-core/src/{protocol,application,storage}.rs`, `storage/recovery.rs`, desktop Tauri/React/CSS/tests, this packet, and `PROGRESS.md`.

## Relevant specifications and ADRs

`docs/content-model.md`, `docs/architecture.md`, `docs/threat-model.md`, ADR 0009, ADR 0014, and `docs/design-direction.md`.

## Exact required behavior

- Accept an active Category ID, title, and summary; generate Guide ID, timestamp, operation ID, filename, initial body, and `draft` status in Rust.
- Create a canonical Markdown Guide at a descriptor-confined direct child path with no-replace recovery journaling and mode `0600`.
- Refresh retained Workspace state and return bounded structured IPC data.
- Expose a labeled keyboard-accessible Reference Desk form; on success open the new Guide in body-edit mode.

## Important states and edge cases

Reject missing/archived Category, invalid title/summary, limits, writer contention, destination conflict, unsafe storage, unsupported durability, interrupted mutation, and committed-refresh failure. Non-ASCII titles use a safe `guide` slug fallback.

## Security constraints

No renderer path, YAML, ID-generation, timestamp, status, operation, shell, Git, or MCP authority. Never overwrite. AI/native creation always yields `draft`.

## Tests to add or update

Core canonical file/path/status/body/mode and validation-no-write tests; native error/payload tests; UI normalization and static semantics.

## Commands to run

`cargo fmt --all`; focused tests; `pnpm check`; `pnpm test`; Workspace Clippy; production/Tauri builds; bootstrap and whitespace checks.

## Explicit non-goals

Metadata editing, Tags/aliases, tested approval, Category move, attachments, deletion, Git, MCP mutation, watchers, and import.

## Definition of done

The new Guide appears after refresh, opens in the editor, remains draft, survives restart, all applicable gates pass, and durable progress names the next slice.

## Expected commit boundary

One signed feature commit, followed by a signed progress-only commit.

## Conditions requiring escalation

Any need for a generic path/YAML API, status elevation, destructive recovery, new dependency, or unresolved filesystem safety tradeoff.
