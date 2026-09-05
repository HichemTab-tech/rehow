# ADR 0004: Derived Field-Aware Search

- Status: Accepted
- Date: 2026-09-05

## Context

Search must stay responsive for several thousand Guides and explicitly rank titles, aliases, commands, paths, and error messages. The index must tolerate malformed content and never become canonical.

## Decision

Use a Rust field-aware full-text index behind `rehow_core::search`, initially Tantivy unless the Phase 4 spike disproves fit. Store it in application XDG cache keyed by Workspace ID/root hash/index format, not among canonical Workspace files. Index plain extracted text by field and layer deterministic product boosts over engine relevance.

Watch events only invalidate; scans/revisions reconcile truth. Rebuild creates a new generation and atomically swaps it. Queries are structured/bounded, not raw engine syntax. Results return plain excerpts plus spans/reasons, never HTML. Ranking fixtures and a 5,000-Guide production benchmark gate changes.

## Consequences

- Deliberate relevance, fast incremental queries, shared desktop/MCP behavior.
- A native dependency and index-version lifecycle.
- Corruption/analyzer upgrade deletes/rebuilds only derived state.
- During rebuild, listing remains available and search exposes progress/bounded metadata fallback.

## Rejected/deferred

- Browser-only index, accidental library-default ranking, canonical SQLite/FTS, and vector/semantic search without demonstrated need.
