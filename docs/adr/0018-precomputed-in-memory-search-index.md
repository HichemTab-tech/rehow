# ADR 0018: Precomputed in-memory search index

- Status: Accepted
- Date: 2026-09-19
- Supersedes: ADR 0004's initial Tantivy presumption; all other ADR 0004 requirements remain active.

## Context

ADR 0004 required a Rust field-aware full-text index and named Tantivy as the initial choice unless the Phase 4 spike disproved its fit. Rehow's supported target is 5,000 Guides / at least 50 MiB Markdown. Adding Tantivy would introduce a native dependency, an on-disk format and generation lifecycle, corruption recovery, analyzer/version compatibility, and duplicate retained text before evidence showed those costs were needed.

The Phase 4B production spike precomputes normalized field documents once per compatible `WorkspaceView`, then runs the existing explicit ranking policy over those documents. Five final release-process runs over 5,000 Guides / 52,665,331 bytes measured worst-case 1.514 s cold inspect/index rebuild, 5.382 ms warm backend p95, and about 140 MiB peak RSS on the recorded reference system. The applicable budgets are 8 s, 50 ms, and 300 MiB.

## Decision

- Use a dependency-free, precomputed in-memory field index behind `rehow_core::search` for the 5,000-Guide v1 target.
- Extract and normalize title, alias, command, path, error, Tag, Category, summary, heading, and body fields once while constructing a compatible Workspace view. Queries never reparse Markdown.
- Keep the accepted deterministic boosts, all-term coverage, archive scope, status tie-breaking, stable ordering, bounded query grammar, and match reasons unchanged.
- Treat the index as disposable derived state. Every successful canonical refresh constructs a new view/index before atomically replacing the retained view; a failed refresh retains the prior pair. No index bytes are written into the Workspace or used as mutation authority.
- Accept bounded `O(total indexed text bytes × terms)` query work at the current 50 MiB/5,000-Guide/16-term ceilings because measured worst-case broad queries remain an order of magnitude inside budget and the simpler design avoids an unnecessary persistence/corruption boundary.
- Revisit an inverted or Tantivy-backed implementation through a new ADR if supported scale increases, warm p95 exceeds 40 ms on the reference corpus, a reproducible regression exceeds 10%, memory approaches 240 MiB, or required single-Guide incremental behavior cannot stay within 150 ms.

## Consequences

Cold rebuild and warm backend budgets pass with substantial margin, ranking remains product-owned rather than engine-default, and deleting process memory loses no content. The index currently rebuilds with the full canonical view; true single-Guide incremental reindex, watchers, persistent cache generations, snippets/highlights, and renderer latency remain explicit Phase 4 closure work.

The benchmark source, hashes, reference environment, exact corpus, command, and five-run results are recorded in `docs/evidence/phase-4b-search-performance.md`.
