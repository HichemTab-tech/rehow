# ADR 0036: Revision-bound MCP Guide Category change

- Status: Accepted for Phase 6C9 implementation
- Date: 2026-09-20

## Context

MCP clients need to organize Guides across Categories without receiving path or filename authority. A retained destination lookup alone is insufficient because the Category could be archived or changed before the recovery writer lock is acquired.

## Decision

Add `guide_change_category` behind the existing per-alias `--allow-write` grant. It accepts only `workspaceHandle`, `guideId`, `expectedRevision`, `categoryId`, and `expectedCategoryRevision`. Rust derives both paths and preserves the exact filename.

Extend the existing journaled Guide-move executor with an exact destination Category-descriptor prerequisite checked under the same recovery writer lock as the source revision and destination-absence checks. The prerequisite becomes part of the plan hash. A same-Category request uses the same lock-scoped pair verification and is a byte-preserving no-op.

The public core operation changes only `category_id`; it preserves title, summary, Tags, aliases, body, ID, filename, lifecycle/verification data except the existing tested-to-stale trust reduction, Attachments, nested data, and unknown metadata. The destination must remain active at the exact supplied descriptor revision.

## Consequences

- Concurrent destination rename/archive/removal conflicts before any Guide write.
- Destination occupancy never overwrites another file; interrupted moves remain recoverable through the existing destination-first journal.
- No caller path, filename, unrelated metadata, generic status, tested approval, delete, Attachment, Git, shell, network, broad, or destructive authority is added.
