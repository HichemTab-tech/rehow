# Phase 5D1 isolated sync inspection evidence

The accepted core boundary derives one configured upstream from the retained repository config, binds it to the current direct branch and complete local state, and consumes the plan into the existing hardened anonymous-HTTPS fetch pipeline. Network Git receives only an explicit canonical URL, exact branch, and exact advertised OID inside Bubblewrap. The active Workspace and repository are never mounted writable—or mounted at all—into that child.

The fetched tree passes the complete clone envelope, independent object hashing, blob streaming, canonical Workspace scan, relationship checks, and Attachment correlation. A different Workspace ID is rejected. A second full local inspection must match the plan after validation. The result exposes no URL, path, ref, OID, Git output, file content, or credential material.

Focused evidence covers exact and duplicate config records, canonical URL/ref rules, redacted plans, changed-config refusal, a locally staged remote snapshot, full validation, and byte-identical active manifest/HEAD after inspection. The existing clone adversarial suite remains the shared evidence for hostile remote trees and content.

Verified checks:

- `cargo fmt --all --check`
- `cargo test -p rehow-core --all-features --locked` — 398 tests passed; 2 explicit network/performance tests ignored; all 43 error-contract, protocol, public-API/trybuild, and unknown-YAML tests passed.
- `cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings`
- `python scripts/check_bootstrap.py`
- `git diff --check`

The single reused Security Reviewer inspected the final authority-bearing diff and returned **PASS** with no blocker, high, or medium finding.
