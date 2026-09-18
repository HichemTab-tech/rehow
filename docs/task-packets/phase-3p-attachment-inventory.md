# Phase 3P — Attachment inventory and correlation

## Objective

Make Workspace opening validate every managed Attachment candidate and correlate Guide metadata with exact confined files before any Attachment mutation is introduced.

## Required reasoning level

Architect/orchestrator, xhigh/high. Attachment filesystem authority requires one final Security Reviewer/xhigh gate.

## Prerequisites

Signed checkpoints `dbf4230`, `0a7856a`, and `b599396`; ADRs 0004, 0009, and 0014; the Attachment sections of the content model and threat model.

## Allowed files or subsystem

`crates/rehow-core`, Cargo lock/manifests, this packet, Attachment-related specification clarifications, and `PROGRESS.md`. No UI, MCP, Git, or write authority.

## Relevant specifications and ADRs

`docs/content-model.md` (Attachments and scan limits), `docs/architecture.md` (capability confinement), `docs/threat-model.md`, ADR 0009, and ADR 0014.

## Exact required behavior

- Treat a missing `attachments/` root as valid only when no Guide declares Attachments.
- Enumerate only `attachments/<guide-id>/<validated-NFC-filename>` through retained directory descriptors; never follow links.
- Count at most 10,000 candidates, at most 64 MiB each, and at most 4 GiB aggregate before reading beyond a limit.
- Stream SHA-256 with bounded memory. Require a regular single-link file and stable device/inode/size/time/mode before and after hashing.
- Correlate every declared Attachment path with one actual file, exact lowercase SHA-256, and optional size. Report missing, orphan, hash mismatch, size mismatch, oversize, unsupported, and unsafe entries without exposing bytes.
- A changed-during-read candidate aborts the snapshot so the prior retained view survives refresh.

## Important states and edge cases

Missing root/file, orphan root/file, malformed Guide metadata (already isolated), invalid directory/filename, symlink, hardlink, FIFO/socket/device, exact/one-over file/count/aggregate limits, hash/size mismatch, and concurrent replacement.

## Security constraints

No absolute path or Attachment bytes cross IPC. No whole-file allocation, opener, decode, preview, write, delete, migration, shell, network, or MCP authority. Diagnostics are fixed kinds plus confined relative paths.

## Tests to add or update

Temporary-Workspace tests for exact correlation, missing/orphan/mismatch, invalid hierarchy, symlink/hardlink/special file, size/count/aggregate boundaries where practical, and changed-during-read behavior. Existing Workspace tests must remain green.

## Commands to run

`cargo fmt --all --check`; focused storage tests; `pnpm check`; `pnpm test`; locked all-target/all-feature Clippy with warnings denied; full all-feature Workspace tests; `pnpm build`; `pnpm tauri:build`; bootstrap and whitespace checks.

## Explicit non-goals

Import, removal, rendering, export, reveal, MIME sniffing, thumbnailing, directory creation, metadata mutation, UI controls, MCP Attachment tools, or search indexing.

## Definition of done

All declared and actual Attachment candidates are safely correlated in a bounded snapshot, adversarial tests and the full gate pass, Security Reviewer reports no blocker/high/medium finding, documentation reflects reality, and a signed checkpoint exists.

## Expected commit boundary

One feature commit for the read-only inventory/correlation slice, followed by one progress-only commit.

## Conditions requiring escalation

Any need to follow links, allocate a whole Attachment, relax limits/path grammar, expose paths/bytes, add write authority, accept unstable metadata, or diverge from the canonical Attachment schema.
