# ADR 0029: Revision-bound MCP Guide creation

- Status: Accepted for Phase 6C2 implementation
- Date: 2026-09-20

## Context

An MCP client needs to create a Guide without choosing filesystem paths, portable IDs, timestamps, or trust status. Category membership is authority-bearing: a stale client must not create inside a Category that was renamed, archived, replaced, or removed after it was read.

## Decision

Add one `guide_create` tool behind ADR 0028's per-alias `--allow-write` grant. Its complete input is `workspaceHandle`, `categoryId`, `expectedCategoryRevision`, `title`, and `summary`. Rehow owns the Guide ID, timestamps, filename, relative path, `draft` status, empty Tags/aliases, and canonical starter body. Clients can subsequently use the separately revision-checked `guide_update` tool to replace editable content.

The core rechecks the exact Category descriptor revision and descriptor-confined current Guide-candidate count while holding the same cooperating writer lock used for the recovery-journaled absent-only Guide create. The prerequisite path and revision are included in the create plan hash. Existing native Guide creation also uses these lock-scoped checks. A stale descriptor or reached 10,000-Guide cap writes no Guide. A committed create followed by refresh failure returns the created ID/revision with `refreshRequired: true`.

## Consequences

- AI-created Guides are structurally unable to begin as `tested`.
- No caller-controlled path, ID, timestamp, body, frontmatter, status, operation ID, shell, network, Attachment, Git, broad, or destructive authority is added.
- Creation intentionally uses the canonical starter body; complete authoring is a second explicit update with the returned revision.
- Cooperating desktop and MCP writers serialize Category changes and Guide creation. Uncooperative direct filesystem changes are detected at the lock-scoped revision read but remain subject to ADR 0009's documented same-user residual race.
