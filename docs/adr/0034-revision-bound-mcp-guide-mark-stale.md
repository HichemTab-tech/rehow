# ADR 0034: Revision-bound MCP Guide mark-stale

- Status: Accepted for Phase 6C7 implementation
- Date: 2026-09-20

## Context

MCP clients must be able to withdraw trust from instructions without receiving generic status authority or a route to assert `tested`.

## Decision

Add a separate `guide_mark_stale` tool behind the existing per-alias `--allow-write` grant. It accepts only `workspaceHandle`, `guideId`, and `expectedRevision`, then delegates to the existing typed core `MarkStale` mutation and recovery journal.

Draft and tested Guides become stale. An already-stale Guide is an exact no-op. An archived Guide remains archived while its restorable status becomes stale. The output therefore includes the current optional restorable status as well as declared/effective status. The tool accepts no generic status or free-form reason because schema v1 has no canonical stale-reason field and MCP cannot create unknown metadata.

## Consequences

- MCP can reduce trust but cannot assert or restore tested trust.
- Exact revisions, atomic recovery, unknown-metadata preservation, and committed-refresh behavior remain shared with existing Guide mutations.
- Delete, body/path/Category changes, generic YAML, Attachment, Git, shell, network, broad, and destructive authority remain absent.
