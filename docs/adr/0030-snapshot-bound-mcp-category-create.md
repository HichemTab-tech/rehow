# ADR 0030: Snapshot-bound MCP Category creation

- Status: Accepted for Phase 6C3 implementation
- Date: 2026-09-20

## Context

Category creation chooses a globally ordered position and is constrained by the 1,000-Category Workspace limit. A retained Workspace view can become stale after any Category is added, edited, replaced, or removed, while the manifest revision alone does not capture those changes.

## Decision

`categories_list` returns an opaque `categorySnapshot` over the Workspace ID, manifest revision, and every valid Category's sorted relative path, ID, and descriptor revision. Add `category_create(workspaceHandle, expectedCategorySnapshot, name, summary?)` behind the existing per-alias `--allow-write` grant. Rehow owns the Category ID, timestamps, directory/path, active status, and next order.

While holding the recovery writer lock, the core counts raw entries in `categories/`, refuses a 1,001st entry, performs a fresh descriptor-confined compatible-Workspace inspection, and compares the complete inventory snapshot before creating an exclusive directory and recovery-journaled descriptor. The expected snapshot is included in the operation plan hash. A committed create followed by refresh failure reports the created ID/revision with `refreshRequired: true`.

## Consequences

- Valid and malformed/residual Category entries cannot bypass the hard limit through a stale retained view.
- Any changed valid Category invalidates creation, even if its order is unaffected; clients refresh and retry deliberately.
- The lock-scoped full Category scan is acceptable because creation is infrequent and the inventory is capped at 1,000.
- No caller-controlled ID, path, order, status, timestamp, YAML, broad lifecycle, destructive, Attachment, Git, shell, or network authority is added.
- Uncooperative same-user writes after the final check retain ADR 0009's documented residual race; Rehow writers serialize on the shared lock.
