# ADR 0035: Revision-bound MCP Guide Tags and aliases

- Status: Accepted for Phase 6C8 implementation
- Date: 2026-09-20

## Context

MCP clients need narrow organization operations that do not require resubmitting a Guide body or unrelated metadata. Tags and aliases have different limits and meaning, so a single ambiguous collection tool would weaken schemas.

## Decision

Add separate `guide_set_tags` and `guide_set_aliases` tools behind the existing per-alias `--allow-write` grant. Each accepts only `workspaceHandle`, `guideId`, `expectedRevision`, and its complete replacement array. The core exposes matching `GuideEdit` variants and remains authoritative for bounds, Unicode case-fold duplicate rejection, round-trip safety, tested-to-stale behavior, atomic recovery, and no-op detection.

Both tools preserve title, summary, body, Category, path, Attachments, lifecycle data except the required tested-to-stale trust reduction, and unknown metadata. Output reports current declared/effective/restorable status so trust changes remain explicit.

## Consequences

- Clients can organize and improve search without broad content replacement.
- Empty arrays intentionally clear the selected collection; ordering is preserved.
- No caller path, Category, body, title, summary, status, tested approval, YAML, Attachment, Git, shell, network, broad, or destructive authority is added.
