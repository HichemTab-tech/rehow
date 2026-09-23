# ADR 0054: Guarded Attachment parent reveal

- Status: Accepted
- Date: 2026-09-23

## Context

Rehow must help users locate managed files, but passing an Attachment to a default application could execute `.desktop` files, scripts, or active documents. Passing a path or URI through renderer IPC would also create disclosure and confused-deputy authority.

## Decision

Rehow may reveal only the Attachment's containing folder. It never asks another application to open the Attachment itself. Renderer IPC supplies only the open Workspace session, Guide ID, exact Guide revision, and Attachment ID. Core resolves the declaration, rereads the Guide around a stable confined source read, enforces the 64 MiB per-file cap, and verifies declared size and SHA-256.

Core then opens every parent component beneath the retained Workspace descriptor with no-follow directory operations and retains that exact directory descriptor. A fixed GIO D-Bus call invokes XDG Desktop Portal `org.freedesktop.portal.OpenURI.OpenFile` with a duplicated Unix file descriptor for the directory. No host path or URI is reconstructed for the handoff. There is no shell, caller-selected executable, argument surface, Attachment descriptor/URI, or renderer-visible path. Errors are closed and content-free.

The native command runs this bounded blocking work off the async IPC executor and permits only one reveal request at a time. The descriptor remains bound to the verified directory even if its pathname or an intermediate component is renamed or replaced after preparation. Rehow retains no authority over the external file manager after the portal accepts the handoff.

The portal method and its `fd` handle semantics are defined by the [XDG Desktop Portal OpenURI interface](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.OpenURI.html).

## Consequences

- Users can locate every verified Attachment without Rehow external-opening it.
- File selection/highlighting is desktop-dependent; the guaranteed behavior is opening the containing folder.
- Linked, special, stale, or metadata-mismatched Attachments fail before handoff; a prepared parent remains inode-bound across later pathname replacement.
- External open-with and Attachment execution remain prohibited.
