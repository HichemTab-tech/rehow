# Phase 5B3B1 Clone Destination Plan Evidence

Date: 2026-09-19

## Boundary

- Core plans only against the retained `EmptyWorkspaceTarget`; it snapshots device/inode, canonical-root hash, launch capability hash, current empty inventory, effective access, and ancestor Git facts.
- Revalidation refuses any new entry or changed/unsafe/nested destination. The remote uses the already accepted secret-free HTTPS/SSH/SCP grammar.
- The Tauri command accepts only `remoteUrl` and returns only `planToken`, `transport`, and `host`. The selected path, URL, root identities, and hashes remain native-held. New selection, cancellation, or local creation invalidates the pending clone plan.
- This slice performs no write, confirmation, Git/process/network/credential operation and is not a usable clone by itself.

## Focused verification

- Core clone-plan tests cover valid/current/changed destinations, invalid credential-bearing URLs, nested repositories, and redacted `Debug`.
- Native payload/error tests prove the closed URL/path-free response and stable content-free mappings.
- The complete relevant gate passed: 327 core tests with 1 intentional production-performance ignore, all 18 error-contract, 13 protocol, public-API/trybuild, 11 unknown-YAML, and 20 desktop Rust tests; formatting and locked all-target/all-feature Clippy for core/desktop passed with warnings denied.
- Bootstrap validation passed with 17 required files, 23 ADRs, and 8 pinned agents; whitespace checks passed.
