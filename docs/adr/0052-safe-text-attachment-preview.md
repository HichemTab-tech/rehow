# ADR 0052: Safe text Attachment preview

- Status: Accepted
- Date: 2026-09-23

## Context

Attachment inventory, import, removal, and MCP metadata are implemented, but the desktop cannot inspect Attachment content. Passing filesystem paths or file URLs to the renderer, trusting extensions/MIME, embedding active formats, or launching an external handler would enlarge the trust boundary unnecessarily.

## Decision

The first Attachment access slice is a pathless, read-only text preview. Renderer IPC supplies only the open Workspace generation, Guide ID, exact Guide revision, and Attachment ID. Native/core code resolves the declared path, rereads the Guide at that revision before and after the Attachment read, opens beneath the retained Workspace capability with no-follow/single-link/regular-file checks, caps input at 1 MiB, and verifies declared size and SHA-256. Missing, stale, changed, linked, special, oversized, hash-mismatched, and invalid-UTF-8 inputs return closed content-free errors.

Preview text is never interpreted as Markdown or HTML. The renderer inserts it only as the value of a scrollable read-only text control. Tab and line feed are retained; control, bidi-formatting, zero-width, BOM, and noncharacter scalars are replaced with U+FFFD and a visible `sanitized` notice. Payload text is capped at 1 MiB before IPC. The UI uses a labeled modal dialog, explicit Close action, Escape, focus containment/restoration, busy state, and polite error/status announcements.

## Consequences

- Users can safely inspect common scripts, configuration, logs, and notes without executing them.
- Binary/image/PDF preview, export, reveal, file URLs, external openers, MIME-based behavior, and MCP content reads remain separate reviewed slices.
- A later export/reveal design must not reuse this renderer payload as filesystem authority.
