# ADR 0001: Rust Core and Thin Adapters

- Status: Accepted
- Date: 2026-09-05

## Context

Desktop Tauri and standalone MCP must enforce identical Workspace, schema, mutation, search, and Git rules. A TypeScript MCP plus Rust desktop backend would duplicate the highest-risk logic; a daemon dependency would make MCP unusable separately; broad filesystem access from the webview would enlarge the attack surface.

## Decision

Use `apps/desktop/` for React/Vite and its Tauri binary at `apps/desktop/src-tauri/`. Use `crates/rehow-core/` with named `domain`, `protocol`, `markdown`, `storage`, `search`, `git`, and `application` modules, plus the standalone `crates/rehow-mcp/` adapter. There is no `rehow-tauri` or separate search crate in the baseline. Extracting modules later requires an ADR.

Tauri and stdio MCP are thin adapters. React/TypeScript owns presentation and draft state only. Return a versioned safe Markdown AST and public DTOs from Rust; generate checked-in TypeScript contracts and fail on drift.

Dependency direction is adapters → application → domain/infrastructure ports. The frontend receives no generic filesystem, process, shell, or URL authority. The MCP binary works without the desktop.

Initial repository boundaries may be consolidated while small, but policy/domain behavior may not live only in an adapter; extraction follows before a second adapter would duplicate it.

## Consequences

- One audited security/behavior implementation and shared conformance tests.
- More Rust code and contract generation than a frontend-heavy Tauri app.
- MCP uses the official [`modelcontextprotocol/rust-sdk`](https://github.com/modelcontextprotocol/rust-sdk) (`rmcp`) with a pinned stable release; Phase 6 still validates protocol/version compatibility against official schemas and real clients.
- UI rendering is predictable but requires a typed AST/component renderer.

## Rejected

- Direct Tauri filesystem/shell plugins from React.
- Separate TypeScript domain implementation for MCP.
- Desktop daemon as MCP's only backend.
- One monolithic command-handler crate with no reusable application boundary.
