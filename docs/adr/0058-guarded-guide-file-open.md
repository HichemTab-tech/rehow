# ADR 0058: Guarded Guide File Open

Status: accepted

Date: 2026-09-24

## Context

Rehow must let a user open a Guide's underlying Markdown file without granting the renderer arbitrary path or shell authority. The action must not disclose an absolute path, follow links, open a stale substitution, or silently choose an external application.

## Decision

- The renderer supplies only the open Workspace session, stable Guide ID, and exact loaded revision.
- Rust resolves the core-owned relative path from the retained Workspace view and opens it component-by-component beneath the retained root.
- The final entry must be a bounded, one-link regular file. Its exact bytes and metadata are read from one held descriptor, its revision must match, and the directory entry must still identify the same inode.
- Native code passes only a duplicated read-only descriptor to XDG Desktop Portal `OpenFile`, with `ask=true` and `writable=false` fixed by Rehow.
- IPC results and errors are pathless. The renderer cannot supply a path, URI, application, writable flag, or portal option.
- Opening is single-flight and off the UI thread; changing Workspace/Guide/revision invalidates the renderer result.

## Consequences

The action safely opens a loaded valid Guide for viewing in a user-selected application. It does not provide a generic opener, expose Workspace paths, execute Guide content, open Attachments, or grant write access. A changed, linked, malformed, oversized, missing, or unsafe file fails closed and asks the user to refresh or repair the Workspace.

## Evidence

Focused core tests cover exact-revision descriptor retention, external changes, and symlink substitution. Native tests cover closed pathless payloads/errors. Strict Clippy, TypeScript/Biome checks, Vitest, native tests, and the production frontend build remain required at the checkpoint.
