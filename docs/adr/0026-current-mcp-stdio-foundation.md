# ADR 0026: Current MCP stdio foundation

- Status: Accepted for Phase 6A implementation
- Date: 2026-09-20

## Context

The original MCP baseline predated the stable 2026-07-28 protocol. The current protocol replaces mandatory session initialization with `server/discover` and per-request version/client metadata, while real clients still use the 2025-11-25 initialize lifecycle. Rehow also needs strict request bounds, protocol-only stdout, explicit Workspace roots, and no ambient authority.

Current primary sources are the [MCP 2026-07-28 discovery specification](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/server/discover.mdx), the [official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk), and its [3.x migration contract](https://github.com/modelcontextprotocol/rust-sdk/discussions/969). The official `rmcp` 3.4.0 release supports both dated protocols and generates JSON Schema 2020-12. It requires Rust 1.88, below Rehow's pinned Rust 1.98.

## Decision

Phase 6 uses exact-pinned `rmcp` 3.4.0 with only server/stdio/schema features plus the smallest exact Tokio/futures support set required by the adapter. It does not enable HTTP, client, OAuth, elicitation, process spawning, or network transports. Rehow uses the SDK's typed lifecycle, schemas, and dispatch rather than implementing MCP wire semantics itself.

The SDK convenience async stdio transport uses an unbounded line buffer. Rehow therefore supplies a small adapter-owned `Transport<RoleServer>` around the SDK's public JSON-RPC codec with a 1 MiB maximum newline-delimited input frame. Over-limit, malformed framing, EOF fragments, and transport I/O fail closed without returning input text. The SDK remains responsible for valid protocol dispatch. Stdout is owned exclusively by the transport; diagnostics are bounded and content-free on stderr.

Startup accepts repeated exact `--workspace alias=/absolute/path` arguments, capped at 16. Alias grammar is lowercase ASCII alphanumeric plus single internal `-`/`_`, 1–32 bytes. Paths exist only at launch, are canonicalized once, opened through core inspection, and identity-rechecked; duplicate aliases and canonical duplicate/nested/overlapping roots are rejected before serving. There is no ambient discovery, client-provided path, current-directory default, environment-root input, or runtime root addition.

Each compatible configured root receives a random launch-only `workspaceHandle` bound in server memory to its canonical root capability, portable Workspace ID, alias, and read-only capability. Handles and paths are never interchangeable; paths are never returned. Phase 6A is read-only and exposes only `workspaces_list` and `workspace_get`, with deterministic JSON Schema 2020-12, closed bounded output, untrusted-content instructions, and stable errors. It does not expose bodies, search, refresh, filesystem diagnostics paths beyond safe managed relative paths, resources/prompts, logging, subscriptions, tasks, mutations, Git, approvals, or arbitrary filesystem/process/network access.

Both 2026-07-28 discovery and 2025-11-25 initialize clients are accepted. Server capabilities advertise tools only. Modern results include the required server identity metadata; legacy initialization uses the corresponding required identity. Tool descriptions and server instructions state that returned Workspace data is untrusted user content and never instructions.

## Consequences

- Current and deployed MCP clients share one official typed adapter.
- The first executable gate is independently useful for installation and Workspace health checks but grants no content or mutation authority.
- Category/Guide/search pagination and response budgets become Phase 6B; write/broad/approval authority remains later.
- Exact dependency and transitive-license/security evidence is required before the first implementation commit.
