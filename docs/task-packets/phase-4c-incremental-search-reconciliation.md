# Task Packet: Incremental search reconciliation

- **Objective:** Reconcile the disposable search index by stable Guide ID and exact revision so a successful canonical refresh rebuilds only changed Guide fields while reusing unchanged indexed text.
- **Required reasoning level:** high for lifecycle integration and acceptance; no new dependency or trust boundary.
- **Prerequisites:** Signed Phase 4B checkpoint `c53378d`; ADRs 0004 and 0018; manual Workspace refresh and mutation-triggered canonical refresh.
- **Allowed files or subsystem:** `crates/rehow-core/src/search.rs`, `crates/rehow-core/src/storage.rs`, this packet/evidence, and `PROGRESS.md`.
- **Relevant specifications and ADRs:** Search sections of `docs/product-spec.md`, `docs/architecture.md`, `docs/testing-strategy.md`; ADRs 0004 and 0018.
- **Exact required behavior:** On successful refresh, correlate by stable Guide ID; reuse the exact indexed allocation only when revision, effective status, and normalized Category name match; rebuild fields for added/changed Guides; omit removed Guides; update cheap status/Category wrappers while sharing unchanged heavy fields; atomically swap view and index only after full confined canonical reconciliation succeeds.
- **Important states and edge cases:** One changed Guide, add, remove, Category rename affecting multiple Guides, archived/effective-status change, malformed isolated Guide, failed refresh retaining the prior view/index, and unchanged Guide identity reuse.
- **Security constraints:** Revisions and loaded canonical records—not file events or index state—decide reuse; the index remains read-only derived memory; no path, watcher, process, network, database, or mutation authority is added.
- **Tests to add or update:** Pointer-identity evidence for unchanged/rebuilt/shared-heavy-field paths; add/remove/Category rename and malformed isolation; ignored release gate with 5,000 Guides / over 50 MiB and exactly one changed Guide.
- **Commands to run:** Focused search tests; core/Workspace Clippy and tests; explicit five-run release incremental gate; bootstrap and whitespace checks.
- **Explicit non-goals:** Filesystem watcher dependency, event-driven targeted canonical reads, persistent cache, UI snippets/highlights/scopes, renderer latency, or replacing full descriptor-confined scan reconciliation.
- **Definition of done:** Every lifecycle case passes, exactly 4,999 of 5,000 document allocations are reused for one changed Guide, five production runs remain below 150 ms, full scan remains the canonical authority/fallback, and independent review has no blocker/high/medium finding.
- **Expected commit boundary:** Index reconciliation, storage refresh integration, tests, packet, and performance evidence form one signed feature checkpoint; verified progress follows.
- **Conditions requiring escalation:** Reuse could cross a revision/status/Category change, a failed refresh would alter retained search results, the 150 ms gate fails, or automatic watching/new dependencies become necessary.
