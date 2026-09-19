# Phase 5B3A Clone Tree Envelope Evidence

Date: 2026-09-19

## Accepted boundary

- The new boundary is crate-private and pure. It accepts only a commit ID plus already captured `ls-tree` bytes and performs no filesystem, process, network, credential, renderer, logging, or mutation operation.
- SHA-1 and SHA-256 identifiers must be full-width lowercase hex and consistent throughout one snapshot.
- NUL records are exact, byte-sorted, unique, and limited to regular non-executable blobs. Paths are classified by the existing Workspace grammar plus optional `.gitignore`; every other path or Git-active file is refused.
- Existing Workspace per-file/count/aggregate limits are reused. A domain-separated BLAKE3 hash binds the commit and ordered materialization effects. Custom `Debug` omits paths, object IDs, and hashes.

## Focused verification

- `cargo test -p rehow-core git::tests::clone_tree --locked`: 6 focused tests passed, including exact/one-over resource envelopes and output from an isolated real Git repository.
- `cargo test -p rehow-core --all-features --locked`: 325 tests passed, 1 intentional production-performance test ignored; all 18 error-contract, 13 protocol, public-API/trybuild, and 11 unknown-YAML tests passed.
- `cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo fmt --all --check`, bootstrap validation (17 required files, 23 ADRs, 8 pinned agents), and whitespace checks passed.

## Deferred acceptance work

This evidence does not claim clone completion. Phase 5B3B must add the confirmed isolated fetch, exact SSH-agent/HTTPS-helper provenance, blob streaming and complete content/schema validation. Phase 5B3C must add marker-bound recovery, one-journal materialization, normal scanner equivalence, config/ref CAS, native interaction, cancellation, and the single independent Security Reviewer gate.
