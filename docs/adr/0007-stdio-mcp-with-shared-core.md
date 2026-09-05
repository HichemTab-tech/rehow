# ADR 0007: Stdio MCP with Shared Core

- Status: Accepted
- Date: 2026-09-05

## Context

MCP must run independently, preserve the desktop's safety semantics, and support future AI mutation without arbitrary host access. Per-call root paths and write-enabled defaults are incompatible with confinement.

## Decision

Ship a standalone Rust `rehow-mcp` stdio binary linking the same application/core crates and the pinned stable official [`rmcp` Rust SDK](https://github.com/modelcontextprotocol/rust-sdk). Startup configuration gives each canonical, non-overlapping root a unique alias and per-Workspace capability flags. The server returns random launch-scoped handles; portable Workspace IDs are metadata and may collide across clones. Duplicate/nested roots or aliases are rejected. Default mode is read-only. Stdout is protocol-only; redacted logs use stderr.

Read tools are paginated/bounded. All mutations require per-Workspace startup write authority and expected revisions; broad/destructive actions additionally require per-Workspace destructive authority, dry-run, and short-lived single-use tokens bound to handle/root identity, operation, effect set, and revisions. AI content always starts draft. MCP cannot complete the trust assertion to `tested`; it can create a pending approval that only the native user interface completes. Git tools require separate per-Workspace Git authority and appear only after their Phase 5 core operations pass security review.

Use stable structured domain errors in tool results and protocol errors only for RPC/schema failure. Treat returned Guide content as untrusted data. Initial transport is local stdio; remote HTTP is deferred.

## Consequences

- Desktop is not required; behavior and tests are shared.
- `rmcp`/protocol-version compatibility needs Phase 6 validation against official clients/spec despite using the official SDK.
- Callers cannot dynamically browse arbitrary roots.
- Write clients must manage revisions, plans, and explicit capability configuration.

## Rejected

- MCP proxy through a running desktop, arbitrary `workspace_path` tool input, write by default, shell/filesystem tools, Node duplication, and unauthenticated HTTP listener.
