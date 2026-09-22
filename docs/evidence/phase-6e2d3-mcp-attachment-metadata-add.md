# Phase 6E2D3 evidence: managed Attachment metadata add

Date: 2026-09-22

## Implemented boundary

- `attachment_candidates_list` scans only `attachments/<GuideId>/` through retained descriptors and returns opaque five-minute candidate tokens, untrusted filenames, decimal sizes, the exact Guide revision, and authenticated pagination.
- Discovery skips declared paths, invalid filenames, symlinks, hardlinks, special files, and oversized files. Existing count, per-file, and aggregate Attachment limits remain authoritative.
- The process-local cache is capped at 256 entries. Tokens are scoped to one Workspace handle, Guide ID, and Guide revision, and are removed before every add attempt.
- `attachment_metadata_add` requires startup write authority and accepts no path, bytes, hash, size, ID, timestamp, media type, YAML, status, shell, process, network, Git, broad capability, or approval assertion.
- The core reopens and streams the exact confined file twice under the cooperating writer lock, comparing device, inode, size, timestamps, mode, link count, SHA-256, and streamed size before the journal boundary.
- Rust generates the Attachment ID, managed relative path, SHA-256, size, and trusted timestamp. Only Guide metadata changes; the candidate file is not copied, renamed, deleted, opened externally, rendered, or executed.
- The existing typed mutation preserves unknown metadata, refuses duplicate content and the 256-Attachment limit, and changes a Tested Guide to Stale.

## Focused verification

- `cargo test -p rehow-core --lib`: 453 passed, 2 intentional ignores.
- `cargo test -p rehow-mcp --lib`: 92 passed.
- Focused tests cover successful add/replay, schema closure, symlink/hardlink/FIFO/oversize filtering, output non-disclosure, read-only refusal, drift, expiry, scope, eviction, archived refusal, duplicate refusal, Tested-to-Stale, and unknown-metadata preservation.
- `cargo clippy -p rehow-core -p rehow-mcp --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- `git diff --check`: passed.

## Review and final gate

The single Security Reviewer initially identified missing core Workspace/root binding and incomplete phase-specific adversarial evidence. Candidates now bind Workspace ID, root hash, and retained capability hash; the expanded tests cover the new refusal branches. Correction re-review: PASS with no blocker, high, or medium finding.

The consolidated gate passed `pnpm check`, `pnpm test`, `pnpm test:integration`, `pnpm build`, Cargo formatting, strict all-Workspace/all-target/all-feature Clippy, Bootstrap validation (17 required files, 49 ADRs, 8 pinned agents), and whitespace checks using the existing Node 26.2.0/pnpm 11.3.0 toolchain.
