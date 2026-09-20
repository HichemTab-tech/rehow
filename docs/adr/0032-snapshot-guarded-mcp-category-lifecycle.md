# ADR 0032: Snapshot-guarded MCP Category archive and restore

- Status: Accepted for Phase 6C5 implementation
- Date: 2026-09-20

## Context

Category archive/restore is reversible but affects whether new Guides may be created. Archiving the Workspace default Category is forbidden. An exact Category descriptor revision alone cannot detect a concurrent manifest change that makes the Category default.

## Decision

Add separate `category_archive` and `category_restore` tools behind the existing per-alias `--allow-write` grant. Both accept only `workspaceHandle`, `categoryId`, and `expectedRevision` and delegate to the existing typed lifecycle mutation. No caller status Boolean is accepted.

Strengthen every typed Category descriptor update by deriving the complete Category inventory snapshot from the retained view and rechecking it through a fresh descriptor-confined inspection while holding the recovery writer lock. The snapshot includes the manifest revision, so a concurrent default-Category change conflicts before mutation. Bind the snapshot into the journal plan. Archive retains the existing default-Category refusal; restore returns the Category to active. Neither operation moves the directory or Guides.

## Consequences

- Category details and lifecycle mutations conservatively conflict on any concurrent manifest or valid Category inventory change.
- Cooperating writers cannot archive a Category that became default while waiting for the lock. ADR 0009's same-user residual race after the final check remains.
- Separate tools keep lifecycle intent schema-bound and add no generic status, YAML, path, reorder, remove, Git, Attachment, shell, or network authority.
