# Phase 4B search performance evidence

Date: 2026-09-19

## Scope

`crates/rehow-core/examples/search_benchmark.rs` deterministically creates 5,000 valid draft Guides containing 52,665,331 bytes of Markdown/YAML (over 50 MiB), excludes generation and release compilation from timings, measures compatible Workspace inspection plus in-memory index construction as the cold rebuild, warms four query shapes, then records 400 backend queries and Linux `VmHWM`. It uses generated temporary content only and removes the temporary Workspace.

This evidence covers cold rebuild, warm ranked backend response, and process peak RSS. It does not claim the visible-render p95, browser main-thread, or single-Guide incremental-reindex budgets; those remain explicit Phase 4 closure work.

## Reproducibility identity

- Command: `cargo run --release -p rehow-core --example search_benchmark --locked`
- Base commit: `f4b642e7563449643f98ed48eeed74d807442e3f` (the signed feature commit is cross-linked from `PROGRESS.md` after acceptance)
- `crates/rehow-core/src/search.rs` SHA-256: `cb540f61391634a9e426b9645ab8d519dbdb78a68fbe5e5bef3314e59ade7ef0`
- `crates/rehow-core/src/storage.rs` SHA-256: `4fdaef3ae5091922a50cb7dac091d4e8f8538132038aef9252ebc54a9300739d`
- `crates/rehow-core/examples/search_benchmark.rs` SHA-256: `7c6d1d62cc002013fb7e87123061133a8b984bf83f7f7a0f7d2543c238cb5708`
- Corpus generator: deterministic sequential indexes `0..4999`; it uses no RNG and therefore has no seed.

## Reference environment

- CPU: AMD Ryzen 7 7700, 8 cores / 16 threads
- Memory: 30 GiB
- OS/kernel: Arch/Omarchy Linux 7.2.5-3-omarchy, x86_64
- Workspace filesystem: Btrfs, zstd:3 compression, SSD
- Rust/Cargo: 1.98.1
- Build: Cargo `--release`, locked Workspace dependencies

## Results

Five independent production-process runs:

| Run | Cold rebuild | Warm p50 | Warm p95 | Warm p99 | Peak RSS |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1513.412 ms | 4.531 ms | 5.367 ms | 5.615 ms | 143,768 KiB |
| 2 | 1510.499 ms | 4.516 ms | 5.293 ms | 5.587 ms | 143,772 KiB |
| 3 | 1512.330 ms | 4.518 ms | 5.363 ms | 5.574 ms | 143,656 KiB |
| 4 | 1513.705 ms | 4.517 ms | 5.382 ms | 5.529 ms | 143,640 KiB |
| 5 | 1502.604 ms | 4.494 ms | 5.326 ms | 5.505 ms | 143,692 KiB |

Worst observed values remain below the applicable product budgets:

- cold rebuild: 1.514 s versus 8 s;
- warm backend p95: 5.382 ms versus 50 ms;
- peak RSS: 143,772 KiB (about 140 MiB) versus 300 MiB.

The benchmark is a reproducible local gate, not a universal hardware guarantee. A regression over 10% requires investigation under `docs/testing-strategy.md` even while absolute budgets still pass.
