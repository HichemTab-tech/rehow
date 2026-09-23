# ADR 0053: Safe Attachment export

- Status: Accepted
- Date: 2026-09-23

## Context

Text preview cannot serve binary Attachments or provide a portable copy. Export necessarily writes outside the Workspace, but a renderer-supplied path would create arbitrary filesystem authority and an ordinary copy could overwrite content or leave a partial destination.

## Decision

The native application owns a one-shot Save dialog and passes its selected destination directly to the Rust core; no path enters or leaves renderer IPC. The request remains bound to the open Workspace session, Guide ID, exact Guide revision, and Attachment ID. Core resolves the declared Attachment internally, rereads the exact Guide revision before and after the source read, requires a stable regular single-link managed file, caps it at the Workspace's 64 MiB per-Attachment limit, and verifies declared size and SHA-256.

The selected destination's parent is opened with Linux `openat2` and `RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS`. Core writes a private unnamed same-directory inode, flushes it, and publishes it with `linkat(AT_EMPTY_PATH)`, which is absent-only. Existing destinations, symlink leaves, and races are never overwritten. The containing directory is flushed after publication; inability to establish these guarantees fails closed, with a distinct result if publication occurred but directory durability could not be confirmed.

The UI exposes a named Export action with busy, cancellation, success, and actionable error states. Export never executes, renders, opens, reveals, or interprets Attachment bytes.

## Consequences

- All declared Attachment formats up to 64 MiB can be copied without exposing host paths to the renderer.
- Existing files require the user to choose another name; Rehow never offers an overwrite confirmation.
- Filesystems without the required unnamed-inode and durable-directory primitives are reported unsupported.
- Reveal and external-open workflows remain separate reviewed slices.
