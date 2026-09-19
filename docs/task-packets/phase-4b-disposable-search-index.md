# Task Packet: Disposable in-memory search index

- **Objective:** Move normalization and Markdown field extraction out of the per-query path into a disposable index rebuilt with each canonical Workspace view, and record repeatable 5,000-Guide production evidence.
- **Required reasoning level:** high for integration and acceptance; implementation is bounded and dependency-free.
- **Prerequisites:** Accepted Phase 4A ranking policy and fixtures; compatible `WorkspaceView` loading; product performance budgets.
- **Allowed files or subsystem:** `crates/rehow-core/src/search.rs`, `crates/rehow-core/src/storage.rs`, one core example benchmark, this packet/evidence, and `PROGRESS.md`.
- **Relevant specifications and ADRs:** Search sections of `docs/product-spec.md`, `docs/architecture.md`, `docs/implementation-plan.md`, and `docs/testing-strategy.md`.
- **Exact required behavior:** Build normalized field documents once after safe Workspace inspection; query only that derived state; preserve exact ranking, archive scope, limits, stable ordering, and match reasons; rebuild after every successful internal or external refresh; malformed Guides stay excluded without blocking healthy results; deleting runtime index state cannot lose content.
- **Important states and edge cases:** Empty/oversized/too-many-term queries, archived scope, malformed isolated Guides, external content change before/after refresh, zero-result queries, 100-result cap, and deterministic ties.
- **Security constraints:** No new filesystem/process/network authority in product code; no canonical database; no user content in logs; benchmark uses generated temporary content only and removes it; index cannot become write authority.
- **Tests to add or update:** Existing ranking fixtures remain unchanged; add lifecycle evidence for stale-before-refresh/rebuilt-after-refresh and malformed isolation; deterministic production corpus covers 5,000 Guides and approximately 50 MB Markdown.
- **Commands to run:** Focused search tests and Clippy; `cargo run --release -p rehow-core --example search_benchmark --locked`; full formatting, Workspace Clippy/tests, bootstrap, and whitespace gates.
- **Explicit non-goals:** Persistent on-disk cache, file watching, snippets/highlights, renderer latency, UI virtualization, related/duplicate search, and true single-Guide incremental reindex. Those remain Phase 4 closure work.
- **Definition of done:** Query ranking is unchanged, queries no longer parse Markdown, refresh rebuild is proven, five production benchmark runs meet cold rebuild/warm backend/RSS budgets, evidence records the reference environment, and documentation states uncovered budgets honestly.
- **Expected commit boundary:** Core index/lifecycle, deterministic benchmark, packet, and evidence form one signed feature checkpoint; verified progress follows.
- **Conditions requiring escalation:** Ranking order changes, budget failure, canonical content would depend on the index, a dependency is proposed, or safe refresh can no longer atomically retain the prior view.
