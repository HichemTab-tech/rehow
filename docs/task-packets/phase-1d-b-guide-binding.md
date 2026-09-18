# Task Packet: Phase 1D-B Guide read-only binding

- **Objective:** Bind accepted Guide frontmatter and opaque body bytes into the public read-only Guide aggregate while retaining private unknown-metadata partitions.
- **Required reasoning level:** high because URL, Unicode, status, and Attachment validation are trust boundaries.
- **Prerequisites:** Phase 1C parser at `cf8341f`; Phase 1D-A binders at `4d26e8f`; dependency audit in `docs/evidence/phase-1d-b-dependencies.md`.
- **Allowed files or subsystem:** `Cargo.lock`, `crates/rehow-core/Cargo.toml`, `crates/rehow-core/src/domain.rs`, `crates/rehow-core/src/domain/content.rs`, `crates/rehow-core/src/domain/content/guide.rs`, `crates/rehow-core/src/parsing/reconcile.rs`, `crates/rehow-core/tests/ui/`, ADR 0014, this packet, and dependency evidence.
- **Relevant specifications and ADRs:** `docs/content-model.md` Guide schema; ADR 0013 parser contract; ADR 0014 read-only surface, binding rules, and Phase 1D-B dependency gate.
- **Exact required behavior:** Expose accessor-only Guide/nested records; bind schema-v1 required and optional fields; retain exact body bytes; preserve absent versus explicitly empty optional collections; retain unknown root and nested entry identities privately; validate limits, types, IDs, timestamps, statuses, Unicode fold uniqueness, URLs, and Attachment metadata.
- **Important states and edge cases:** Draft/tested/stale/archived companion matrices; malformed/null values; exact/over bounds; case-fold collisions without normalization; noncanonical URLs and IP spellings; Attachment traversal, NFC, MIME, digest, size, duplicate ID/path; self/duplicate related Guides; read-only parser artifacts; source positions.
- **Security constraints:** No filesystem/network/process access; URLs are validated but never opened; Attachment paths are inert confined relative metadata; failures contain no rejected content; aggregate construction/mutation and Serde remain private.
- **Tests to add or update:** Production-backed unit tests in the Guide binder and trybuild pass/fail cases proving the accessor surface and private construction/mutation/serialization.
- **Commands to run:** `cargo fmt --all --check`; `cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings`; `cargo test -p rehow-core --all-features --locked`; `cargo tree -p rehow-core --locked -e features`.
- **Required reviews and artifacts:** One combined independent correctness/security review; dependency evidence recording package identity, feature graph, source/unsafe/build review, yank/advisory status, and lock result.
- **Explicit non-goals:** Mutation planning/emission (1D-C), filesystem inventory/storage (1E), effective tested trust/hash recomputation (Phase 3), opening URLs, or reading Attachment files.
- **Definition of done:** All specified behavior exists; tests and lint pass; dependency evidence is current; review has no unresolved blocker; documentation states the actual acceptance boundary.
- **Expected commit boundary:** One implementation/evidence commit, followed by a progress-ledger commit after verification.
- **Conditions requiring escalation:** Dependency audit failure, inability to meet the frozen Unicode/URL rules, public mutation authority, content-bearing errors, filesystem authority, or a schema contradiction.
