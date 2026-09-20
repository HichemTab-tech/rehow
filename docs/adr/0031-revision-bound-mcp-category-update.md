# ADR 0031: Revision-bound MCP Category details update

- Status: Accepted for Phase 6C4 implementation
- Date: 2026-09-20

## Context

MCP clients need to correct a Category's user-visible name and optional summary without receiving generic YAML, filesystem, ordering, or lifecycle authority.

## Decision

Add `category_update(workspaceHandle, categoryId, expectedRevision, name, summary?)` behind the existing per-alias `--allow-write` grant. It delegates to the existing typed core Category-details mutation and recovery journal. The exact descriptor revision is checked before and under the cooperating writer lock. Rehow preserves the Category ID, directory/path, order, status, creation time, Guides, and unknown metadata; only name, optional summary, and the core-owned update time may change.

Success returns the resulting revision, changed/no-op state, and refreshed complete Category snapshot. A committed mutation followed by refresh failure is reported as committed with `refreshRequired: true`. Conflicts refresh the retained view and return the actual correlated revision when available.

## Consequences

- No generic patch/YAML, path, order, status, lifecycle, deletion, Git, Attachment, shell, or network authority is added.
- Safe no-ops preserve the revision and inventory snapshot.
- Presentation that cannot be changed without risking comments or unknown metadata remains read-only and fails explicitly.
