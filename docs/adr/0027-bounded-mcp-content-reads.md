# ADR 0027: Bounded MCP content reads

- Status: Accepted for Phase 6B implementation
- Date: 2026-09-20

## Context

Phase 6A proves protocol and startup authority but exposes no Guide content. Phase 6B must make the standalone server useful without turning reads into an unbounded disclosure or adding mutation authority. Workspaces may contain thousands of Guides, Guide bodies may reach the canonical 2 MiB limit, and external editors may change files while the server runs.

## Decision

Phase 6B adds only `categories_list`, `category_get`, `guides_list`, `guides_search`, `guide_get`, `workspace_validate`, and `workspace_refresh`. Every tool requires the launch-only `workspaceHandle`; no tool accepts a path. The existing retained descriptor and core `WorkspaceView` remain the only content authority. `workspace_refresh` invokes the existing read-only confined refresh and atomically retains the prior view on failure.

List/search/validation tools use a default page size of 25 and a hard maximum of 100. Their adapter cursors are versioned, tool/filter/workspace-bound, offset-based over the immutable retained view, and authenticated with a per-launch BLAKE3 key so callers cannot alter the offset. They remain pagination tokens, not mutation authority. Invalid, forged, or mismatched cursors fail with stable invalid-parameter errors. Search retains the core 512-byte/16-term/100-result limits and ranking; Category/Tag eligibility is applied inside the shared core search before ranking truncation.

`guide_get` returns typed metadata, revision, effective status, optional UTF-8 Markdown body, and separately extracted fenced code blocks. `maxBodyBytes` defaults to 256 KiB and cannot exceed the canonical 2 MiB Markdown limit. Truncation stops at a UTF-8 boundary and reports total bytes plus `bodyTruncated`; code-block extraction is independently capped at 128 blocks and 128 KiB. No rendered HTML, arbitrary Attachment bytes, external file bytes, absolute paths, Git output, or executable command surface is returned.

Outputs use only bounded core fields and closed discriminants. Relative managed paths in declared `filesAffected`, Attachment metadata, and validation diagnostics are content data and are labeled untrusted. Tool descriptions and server instructions continue to state that all returned Workspace data is untrusted and never instructions.

## Consequences

- Agents can retrieve and search useful Guide data while the server remains read-only.
- Pagination is deterministic for one retained view; a successful refresh intentionally invalidates old cursors through the manifest/view fingerprint.
- Duplicate portable Workspace IDs are recomputed from one coherent read-lock snapshot, including after refresh.
- Tag filters use exact stored Tag spelling in this gate; richer normalized filter matching may be added only with a shared core semantic API.
- Category/Guide mutations, raw Attachment transfer, Git tools, resources/prompts, and approval behavior remain absent.
