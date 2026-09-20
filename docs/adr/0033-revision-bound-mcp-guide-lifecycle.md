# ADR 0033: Revision-bound MCP Guide archive and restore

- Status: Accepted for Phase 6C6 implementation
- Date: 2026-09-20

## Context

MCP clients need reversible Guide lifecycle controls without generic status authority or the ability to claim that instructions were tested.

## Decision

Add separate `guide_archive` and `guide_restore` tools behind the existing per-alias `--allow-write` grant. Each accepts only `workspaceHandle`, `guideId`, and `expectedRevision` and delegates to the existing typed core lifecycle mutation and recovery journal. Neither accepts a status value.

Archive preserves the Markdown path, body, Category, Attachments, and unknown metadata while retaining the previous restorable status in canonical metadata. Restore follows the core's closed lifecycle matrix; it cannot directly assert `tested`, and legacy/tested restoration follows the existing safe stale-state policy. Conflict handling refreshes the retained view and returns the current correlated Guide revision when available.

## Consequences

- Lifecycle intent is explicit and reversible; deletion remains absent.
- AI/MCP cannot use restore to bypass native tested confirmation.
- No generic YAML, body, path, Category move, tested approval, delete, Attachment, Git, shell, or network authority is added.
