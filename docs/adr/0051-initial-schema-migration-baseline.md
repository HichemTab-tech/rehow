# ADR 0051: Initial schema migration baseline

- Status: Accepted
- Date: 2026-09-23

## Context

Rehow schema v1 is the first released portable Workspace format. Phase 2 requires explicit compatibility and migration behavior, but there is no genuine older Rehow schema or deployed content to transform. Inventing a synthetic v0 would create an unearned compatibility promise and destructive code with no real input contract. Ordinary Markdown is not a legacy Rehow Workspace and is already handled by the safe copy-import boundary in ADR 0050.

## Decision

The v1 supported-older-version registry is empty. A positive manifest version below the current minimum would be classified as older only after an ADR defines its exact source schema and deterministic migration; no such version exists for v1. Zero, negative, missing, non-integer, and out-of-range versions remain malformed. Versions above the application maximum remain newer-unsupported and read-only.

Rehow exposes no migration apply action while the registry is empty. The first future schema change must add, in one reviewed boundary, the exact old-version binder, read-only plan/effect set, verified portable backup inventory and completion marker, revision/concurrency checks, journaled transform, post-validation, explicit native confirmation, and restore evidence required by ADRs 0002 and 0009. Reading never migrates or creates identifiers.

V1 ships copy import for ordinary Markdown. In-place Markdown adoption remains deferred: Rehow does not silently add metadata, move files, or reorganize a nonempty folder. This decision resolves the Phase 2 adoption question without weakening the future migration contract.

## Consequences

- Current, newer-unsupported, malformed, ordinary-Markdown, and unrelated-folder classifications remain truthful; there is no unreachable “older supported” UI promise.
- The compatibility registry can gain an older-supported entry only alongside a complete migration ADR and fixtures.
- No unused mutation or backup implementation is added before a real source schema exists.
- Copy import is the safe v1 path for bringing existing Markdown into Rehow.
