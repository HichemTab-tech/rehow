# Phase 4C incremental search evidence

Date: 2026-09-19

## Behavior

`WorkspaceView` refresh still performs the complete descriptor-confined canonical scan. Only after that scan succeeds does `SearchIndex::reconcile` compare stable Guide IDs, exact path-sensitive revisions, effective status, and normalized Category name. It reuses unchanged `Arc<SearchDocument>` allocations, rebuilds changed Guide fields, adds/removes documents, and creates cheap wrappers sharing the same `Arc<SearchFields>` when only status or Category display changes. The refreshed view/index replace the retained pair atomically; any scan failure keeps the prior pair.

Automatic filesystem watching is not claimed. Internal mutations and the existing manual external refresh both use this reconciliation path; watcher events and targeted canonical rereads remain later Phase 4 work.

## Production gate identity

- Command: `cargo test --release -p rehow-core single_guide_incremental_reindex_meets_production_budget --all-features --locked -- --ignored --nocapture --test-threads=1`
- Base commit: `95b29255a0b02aa21a31868b8b458199a7f2ce8d`
- `crates/rehow-core/src/search.rs` SHA-256: `fad51db45b69f4dd5390b4cc9227d90df94af73993ad3c01f56ce78421e0e509`
- `crates/rehow-core/src/storage.rs` SHA-256: `896ff59dcb0226a8ae253163c10e9df0f1e3b495c86b7fc5a98d6b77af2eb3ac`
- Corpus: deterministic sequential indexes `0..4999`, no RNG/seed, 5,000 Guides, the same over-50-MiB shape as Phase 4B, exactly Guide 02500 changed.
- Reference environment: the same AMD Ryzen 7 7700 / 30 GiB / Btrfs / Linux / Rust 1.98.1 system recorded in `phase-4b-search-performance.md`.

## Results

Five independent release-test processes measured the production reconciliation call:

| Run | One-Guide incremental reindex | Reused document allocations |
| ---: | ---: | ---: |
| 1 | 1.317 ms | 4,999 / 5,000 |
| 2 | 1.298 ms | 4,999 / 5,000 |
| 3 | 1.249 ms | 4,999 / 5,000 |
| 4 | 1.261 ms | 4,999 / 5,000 |
| 5 | 1.234 ms | 4,999 / 5,000 |

The five-run worst was 1.317 ms versus the 150 ms product budget. Corpus generation and the second full canonical inspection occur outside the timed region by design: this gate measures incremental reindex, while Phase 4B separately gates cold full inspection/index rebuild.

After review added only the missing archive-transition coverage test, the benchmark was repeated against the final recorded source hash: 1.424 ms with 4,999 / 5,000 allocations reused. The original five-run set above measures the same production implementation.
