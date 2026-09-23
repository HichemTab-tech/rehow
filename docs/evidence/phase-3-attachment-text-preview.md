# Phase 3 Attachment text preview evidence

Date: 2026-09-23

## Result

The desktop can preview a declared Attachment as inert UTF-8 text without exposing a host path, file URL, arbitrary-read input, or external opener. The request is bound to the open Workspace session, Guide ID, exact Guide revision, and Attachment ID. Core rereads the on-disk Guide before and after the Attachment read, resolves the declared path internally, enforces retained-root/no-follow/single-link/regular-file/stable-read checks, caps input and output at 1 MiB, and verifies declared size and SHA-256.

Binary, oversized, missing, stale, linked, special, changed, and metadata-mismatched inputs fail with closed content-free errors. Unsafe control, Unicode 16 Format/Default-Ignorable, bidi, zero-width, BOM, variation-selector, tag, and noncharacter scalars are replaced before IPC; the renderer independently rejects unsanitized payloads with the same policy. Content is displayed only as a read-only text-control value and is never parsed as Markdown/HTML or used as navigation/process authority. Debug output omits content.

## Interaction and design review

The project-pinned `web-design-guidelines` skill was reread and its current upstream rules were fetched on 2026-09-23. The review corrected modal overscroll containment, Close hover feedback, Title Case, balanced/long title handling, narrow wrapping, and keyboard-scrollable read-only content. Native modal behavior provides Escape, focus containment, and first-control focus; closing restores the originating Attachment action. Async completion is generation/owner checked against the Workspace session, Guide ID, and exact Guide revision, so stale content cannot remain or reopen after a Workspace, selection, or revision change. The bounded modal does not alter the accepted shell geometry, so this slice does not require a separate shell screenshot checkpoint.

## Verification

- Four focused core tests pass: safe/sanitized text, stale/binary/content drift, symlink/hardlink/FIFO/oversize refusal, and post-read Guide-race refusal.
- Desktop Rust passes 29 tests, including closed/pathless payload and stable error mapping.
- Vitest passes 44 tests, including strict response shape/Unicode-policy rejection, Workspace/revision owner binding, and inert HTML/file-URL-looking text rendering.
- The full repository gate passes: 480 core tests (2 intentional ignores), 18 error-contract tests, 13 protocol tests, public-API/trybuild and unknown-YAML suites, 29 desktop Rust tests, 92 MCP tests, 3 approval-helper integration tests, 1 stdio integration test, strict Clippy, Cargo formatting, `pnpm check`, production Vite/Cargo build, Bootstrap validation (17 required files, 52 ADRs, 8 pinned agents), and whitespace checks.
- The correction-only Security Reviewer verdict is **PASS** after Workspace/revision ownership and the complete Unicode policy were verified.
- Existing Node 26.2.0 and pnpm 11.20.0 were used without installation or switching.

No host path, arbitrary filesystem read, shell/process launch, or MCP authority was added.
