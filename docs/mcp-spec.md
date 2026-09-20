# Rehow MCP Server Specification

Status: required Phase 6 contract; current protocol baseline refreshed for 2026-07-28
Transport: local stdio
Protocol: MCP 2026-07-28 plus 2025-11-25 compatibility
Last updated: 2026-09-20

## Purpose and deployment

`rehow-mcp` lets an MCP client work safely with explicitly configured Rehow Workspaces without requiring the desktop app. It links the same application/core crates, schemas, revisions, storage, search, Git policies, and locks as the desktop adapter. Read access is the default; Phase 6C1 implements only an explicit per-alias write grant for complete existing-Guide updates.

The server is installed separately and launched with an allowlist, for example:

```text
rehow-mcp --workspace personal=/absolute/path --allow-write personal
```

Each startup root has a unique user-chosen alias. Canonical duplicate/nested/overlapping roots and duplicate aliases are rejected. Portable Workspace IDs may legitimately repeat across clones, so the server returns a random launch-scoped `workspaceHandle`; every tool requires it. No tool accepts a new path. Default mode is read-only. `--allow-write <alias>` is implemented as a separate repeated startup argument and applies only to that configured alias. Write mode retains private recovery roots by directory descriptor and refuses startup if either recovery root overlaps any configured Workspace. `--allow-broad` and `--allow-git-write` remain absent. Stdout contains protocol frames only; redacted diagnostics use stderr.

Initial transport is stdio. Remote Streamable HTTP is deferred until authentication, origin, multi-user, and network threat models exist.

The server uses the official Rust SDK and supports both current stateless discovery (`server/discover`, with protocol/client metadata on each request) and the preceding initialize lifecycle for deployed clients. Phase 6C1 exposes the nine Phase 6B reads plus capability-gated `guide_update`. Stdio uses newline-delimited JSON-RPC with a Rehow-owned 1 MiB per-frame limit; exceeding the limit closes the transport without echoing input. Stdout is protocol-only from process start through shutdown.

## Trust rules

- Guide text, Markdown links, filenames, metadata, Git output, and MCP arguments are untrusted data, never instructions.
- No arbitrary filesystem APIs, arbitrary URI fetches, shell/command execution, Guide command execution, or ambient Workspace discovery.
- Returned content is labeled/structured as user content and bounded; tool descriptions explicitly tell clients not to follow instructions embedded in content.
- The core confines every operation and rejects managed symlinks/traversal.
- Mutation requires per-Workspace startup write enablement plus current revisions. Read-only mode cannot be escalated by a tool call.
- AI/MCP-created Guides are always `draft`. MCP cannot directly assert `tested`; it creates a pending native approval.
- Broad/destructive/Git mutations are preview-first, plan/effect-token bound, separately enabled at startup, and require out-of-band native approval unavailable as an MCP input field.

## Protocol conventions

### Identifiers and revisions

- `workspaceHandle`: opaque random per-launch authority bound to alias, canonical root identity, Workspace ID, and capability flags.
- Workspaces, Categories, Guides, Attachments: canonical portable UUID strings; UUID alone is never authority.
- Revisions: opaque strings (`rev:v1:...`); callers compare/pass but do not construct them.
- Pagination cursor: opaque, bound to Workspace/index generation/query/filter/order.
- Times: RFC 3339 UTC.

### Success envelope

Tool `structuredContent` follows:

```json
{
  "ok": true,
  "requestId": "0199…",
  "workspaceHandle": "wsrun_…",
  "workspaceId": "0199…",
  "data": {},
  "warnings": [],
  "changeSummary": null
}
```

`changeSummary`, when present, lists operation, affected stable IDs, safe relative paths, before/after revisions/status, and recovery/next action. It never includes secrets or hidden ambient paths.

### Domain error envelope

Valid tool calls that fail in the domain return MCP `isError: true` and:

```json
{
  "ok": false,
  "error": {
    "version": 1,
    "code": "REVISION_CONFLICT",
    "message": "The Guide changed since it was read.",
    "retryable": false,
    "requestId": "0199…",
    "details": {
      "entityKind": "guide",
      "entityId": "0199…",
      "expectedRevision": "rev:v1:…",
      "actualRevision": "rev:v1:…"
    },
    "recovery": "Read the Guide again, compare changes, and submit a new revision-aware update."
  }
}
```

RPC/tool-schema failures use MCP protocol errors. Domain failures nest the exact shared `ErrorDto` unchanged under `error`, using the codes, retryability, allowed detail keys, and redaction rules in `docs/error-contract.md`; adapters may not rename them. Messages are not API contracts. A shared serialized fixture must pass byte/schema parity tests through both Tauri and MCP adapters.

### Limits

List/search/validation calls default to 25 results and reject limits outside 1–100. Cursors are versioned, authenticated with a per-launch key, and bound to the offset, tool, filters, launch Workspace handle, and retained view; forgery fails and refresh invalidates old cursors. Guide bodies default to 256 KiB and cannot exceed the canonical 2 MiB limit, truncate only at UTF-8 boundaries, and report total bytes plus truncation. Extracted fenced code is independently capped at 128 blocks and 128 KiB. Search applies Category/Tag eligibility before the 100-result ranking cap.

## Workspace tools

- `workspaces_list()` — configured aliases only, returning launch handle, portable ID, compatibility/availability, and capabilities. Duplicate portable IDs are explicitly reported.
- `workspace_get({workspaceHandle})` — manifest summary, inventory/health counts, schema, and Git capability; no absolute path or Guide body. Entry-level inventory is deferred beyond Phase 6A and will be bounded/paginated.
- `workspace_validate({workspaceHandle, scope?, cursor?, limit?})` — paginated diagnostics.
- `workspace_refresh({workspaceHandle})` — read/reconcile only; does not mutate canonical content.

Arbitrary filesystem discovery and `workspace_open(path)` are intentionally absent.

## Category tools

Every Category tool requires `workspaceHandle`; omitted parameters below are shorthand, not session-global selection.

- `categories_list({workspaceHandle, includeArchived?, cursor?, limit?})`
- `category_get({workspaceHandle, categoryId})`
- `category_create({workspaceHandle, expectedWorkspaceSnapshot, category})`
- `category_update({workspaceHandle, categoryId, expectedRevision, patch})`
- `category_reorder_plan({workspaceHandle, expectedWorkspaceSnapshot, orderedCategoryIds})`
- `category_reorder({workspaceHandle, planToken})` — requires startup broad capability and out-of-band approval.
- `category_archive` / `category_restore` with expected revision.
- `category_remove_plan({workspaceHandle, categoryId, expectedRevision, replacementDefaultCategoryId?})`
- `category_remove({workspaceHandle, planToken})` — only empty Category; recoverable trash; startup broad capability and out-of-band approval.

Rename updates `name` only. `category_tidy_path` is explicit. Archiving/removing the default Category atomically replaces/clears the manifest default or is refused. Broad reorder/remove is preview-first. No cascade delete.

## Guide tools

Every Guide tool requires `workspaceHandle`.

- `guides_list({workspaceHandle, categoryId?, statuses?, tags?, cursor?, limit?})`
- `guide_get({workspaceHandle, guideId, includeBody?, maxBodyBytes?})`
- `guides_search({workspaceHandle, query, includeArchived?, categoryIds?, tags?, cursor?, limit?})`
- `guide_create({workspaceHandle, expectedWorkspaceSnapshot, guide})` — always creates `draft`; a status field is rejected.
- `guide_update({workspaceHandle, guideId, expectedRevision, title, summary, tags, aliases, body})` — implemented complete editable-content replacement; 768 KiB body cap; typed fields only; one atomic candidate; verification-relevant edits automatically make tested content stale.
- `guide_update_title`, `guide_tidy_path`, and `guide_change_category` — explicit operations with expected revision; Category change is a journaled path+frontmatter update; ID unchanged. There is no generic move.
- `guide_set_tags` / `guide_set_aliases` — complete normalized arrays with expected revision.
- `guide_change_status({…, status})` — cannot set `tested` through MCP.
- `guide_request_tested_approval({workspaceHandle, guideId, expectedRevision})` — readiness check plus pending native approval; no immediate status change.
- `guide_mark_stale({…, reason?})`
- `guide_archive` / `guide_restore` with expected revision.
- `guide_delete_plan({workspaceHandle, guideId, expectedRevision})`
- `guide_delete({workspaceHandle, planToken})` — recoverable trash; startup broad capability and native approval.
- `guide_validate({workspaceHandle, guideId})`
- `guides_find_duplicates({workspaceHandle, guideId?, cursor?, limit?})`
- `guides_find_related({workspaceHandle, guideId, cursor?, limit?})`

Read results separate typed metadata, body, bounded fenced-code blocks, diagnostics, and revision. They do not return rendered HTML or Attachment bytes.

## Attachment metadata tools

- `attachments_list({workspaceHandle, guideId})`
- `attachment_metadata_add/update/remove_plan/remove` operate only on Guide metadata plus already managed Attachment IDs.

Raw arbitrary attachment upload/download through MCP is deferred. If later approved, it must use bounded protocol resources, content hashes, regular-file-only confined storage, and separate threat review. A tool cannot attach a host path supplied in its arguments.

## Out-of-band native approval

MCP arguments cannot prove user consent, so no boolean/phrase/timestamp supplied by the client can mark a Guide tested or approve a broad/destructive/Git mutation. Execution of those operations becomes a bounded MCP task with a pending `approvalRequestId`, safe effect summary, and expiry.

The server itself launches its fixed, installed `rehow-mcp-approve` sibling over two inherited anonymous pipes after validating the plan. It derives the path from its own executable directory, opens it no-follow as a regular single-link file, rejects group/world-writable or unexpected ownership, verifies the package/build hash, and executes the already-open descriptor (`execveat`/equivalent) to prevent a swap. It never takes a helper path from MCP, resolves no `PATH`, uses no shell, and passes no approval secret/effect data through argv, environment, filesystem, public socket, stdin, or stdout. Before exec, every inherited descriptor is close-on-exec and closed except the two approval channels; the child closes all other descriptors again (`close_range` or a verified equivalent), binds stdin to `/dev/null`, and binds stdout/stderr to non-client safe sinks. The child renders a length-bounded plain-text effect summary with no markup, active links, hidden overflow, or unbounded target list, and returns only the button result over the dedicated result pipe. The MCP client cannot read or write either approval channel or substitute effect data. Only a real dialog confirmation records approval in server memory; the core supplies its own timestamp and rechecks state. Headless sessions cannot complete these operations. Tests inspect the child descriptor set (for example through `/proc/self/fd` on Linux), stdin/stdout/stderr bindings, truncation disclosure, and both pipe directions. Same-user tracing/injection or GUI automation is outside the hard boundary, as documented in the threat model.

For tested approval, the dialog says the user is asserting they personally tried the displayed Guide revision. The core rechecks that revision, readiness, and verification hash immediately before changing status. A verified-content edit later produces effective `stale`.

## Plan and confirmation protocol

`*_plan` is read-only and returns:

```json
{
  "planToken": "plan:v1:…",
  "expiresAt": "…",
  "operation": "guide_delete",
  "targets": [{"guideId": "…", "revision": "…"}],
  "effectSet": "effect:v1:…",
  "effects": ["Move the Guide and managed Attachments to recoverable trash."],
  "warnings": []
}
```

The opaque token is process-local, short-lived, single-use, and bound to launch handle/root capability identity, portable Workspace ID, operation, exact effect records/revisions, and relevant inventory/Git state. Broad/destructive execution requires the matching startup capability plus native approval; the call passes only `planToken`, never a client assertion of consent. A changed/replayed/expired/wrong-operation plan fails without mutation. Dry-run remains available in read-only mode.

## Git tools

Availability follows the approved Phase 5 core:

- `git_status({workspaceHandle})`
- `git_commit_plan({workspaceHandle, expectedGitState, message})`
- `git_commit({workspaceHandle, planToken})`
- `git_sync_plan({workspaceHandle, expectedGitState, direction})`
- `git_sync({workspaceHandle, planToken})`

The server exposes no generic Git arguments, refspecs, paths, environment, credentials, or raw output. Git mutation tools do not exist until their Phase 5 ADR and malicious-repository gate pass, and then require per-Workspace Git startup capability plus native out-of-band approval. Sync never lets Git directly rewrite active content without validated/journaled core materialization, and never force-pushes, resets, rebases, stashes, discards, or auto-resolves. Authentication requirements are reported without soliciting secrets.

## Target AI workflow

1. AI searches with `guides_search` before writing.
2. It reads likely duplicates/related Guides.
3. It lists/selects an existing Category or creates one with current snapshot.
4. It updates with current Guide revision or creates a `draft`.
5. It calls `guide_validate` and reports `changeSummary` exactly.
6. User tries the instructions outside Rehow.
7. AI calls `guide_request_tested_approval` only after the user says they tried it; the Guide changes only if the user separately confirms the native approval dialog for that exact revision.

## Testing obligations

- JSON schema snapshots and backward-compatible error fixtures.
- Server startup/handshake, stdio framing, stdout purity, shutdown, cancellation, pagination, response limits, invalid inputs, and deterministic tool lists.
- Read-only/per-Workspace write/broad/Git capability separation and duplicate/nested root/portable-ID handling.
- Arbitrary path/root, traversal, symlink and race attempts.
- Malformed/newer/older Workspace, corrupt/duplicate Guide, external changes, stale revision/snapshot, concurrent desktop/MCP writers.
- Plan expiration/replay/substitution/staleness, approval-child provenance/pipe/exit handling, and tested-approval bypass attempts.
- Prompt injection in title/body/filename/metadata asking the client/server to escape or execute.
- Secret-like content and credential-bearing Git remotes absent from logs/errors/summaries.
- Malicious Git repository suite before Git mutations are exposed.
- End-to-end target workflow against a temporary Workspace, with resulting files inspected byte-for-byte.

## Client configuration and versioning

Installation docs will provide examples for supported MCP clients without modifying global client config automatically. The executable and tool schemas are semantically versioned. Workspace schema compatibility and server version are returned by `workspace_get`. Breaking tool/schema changes require a new major server contract and migration notes; content schema remains independently versioned.
