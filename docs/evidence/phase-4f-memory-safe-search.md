# Phase 4F memory-safe search evidence

Date: 2026-09-23

## Scope

ADR 0057 removes retained Guide bodies and in-memory normalized full text. Guide reads are descriptor-confined and revision-checked. Search records are versioned, length-framed, anonymous, disposable, incrementally reused, and periodically compacted.

## Production benchmark

Command:

```text
cargo run --release -p rehow-core --example search_benchmark --locked
```

Observed on the existing deterministic 5,000-Guide / 52,665,331-byte corpus:

| Metric | Result | Budget |
| --- | ---: | ---: |
| Cold inspect/index | 1,533.060 ms | 8,000 ms |
| Warm p50 | 12.260 ms | informative |
| Warm p95 | 13.484 ms | 50 ms |
| Warm p99 | 14.351 ms | informative |
| Warm limit-1 p95 | 13.381 ms | 50 ms |
| Peak RSS | 10,228 KiB | 300 MiB |

The former in-memory evidence measured about 140 MiB peak RSS. The new result is approximately 10 MiB while exact ranking and excerpts remain unchanged.

The release-only reconciliation command passed at 0.667 ms and reused 4,999 of 5,000 document descriptors:

```text
cargo test --release -p rehow-core search::tests::single_guide_incremental_reindex_meets_production_budget --all-features --locked -- --ignored --nocapture
```

## Focused verification

- The full pre-review core gate passed 489 tests with two explicit production/network tests ignored; the only full-suite failure was a corrected compile-diagnostic snapshot changed by the intentional metadata type split.
- Thirteen focused search tests pass after security corrections; the release-only performance test remains intentionally ignored by the debug suite.
- Codec and backing tests cover embedded NUL/Unicode, exact round-trip, every truncated prefix, invalid UTF-8, trailing/corrupt input, forged/over-limit lengths and counts, positional failures, and pre-write one-over record/physical prior-plus-candidate refusal even when compaction is selected.
- The compaction test advances through the generation threshold, verifies one live backing remains, and confirms the latest content is searchable.
- Workspace-wide `cargo check --all-targets --all-features --locked` passed for core, desktop, and MCP.

Final complete gate results are recorded in `PROGRESS.md` after acceptance.
