# Phase 3 Attachment export evidence

Date: 2026-09-23

## Result

The desktop exports any declared Attachment up to 64 MiB through a native Save dialog without exposing a source or destination path to the renderer. Core binds the request to the open Workspace session, Guide ID, exact Guide revision, and Attachment ID; rereads the Guide around the source operation; requires a stable confined regular single-link source; and verifies declared size and SHA-256.

The destination parent is retained with Linux `openat2` under no-symlink/no-magic-link resolution. Bytes are written to a same-directory unnamed `0600` inode, flushed, and published only if the selected leaf is absent. Existing files and symlinks are preserved. No named partial file exists, and publication-directory flush failure is reported distinctly without deleting the published file.

The renderer receives only `{ kind: "cancelled" }` or `{ kind: "exported", sizeBytes }`. It never receives a path, URL, bytes, process handle, or filesystem capability.

## Interaction and design review

The project-pinned `web-design-guidelines` skill was reread and the current upstream rules were fetched on 2026-09-23. The named native button has visible hover/focus treatment, a specific `Export` label, `Exporting…` busy copy, mutual exclusion with other Guide/Attachment operations, polite success/cancellation/error feedback, actionable overwrite guidance, and narrow-safe wrapping in the existing Attachment row. The native Save dialog returns focus to the invoking control. This bounded row action preserves the accepted shell geometry, so a separate screenshot checkpoint was not required.

## Verification

- Three focused core tests pass: exact private-byte publication/mode and overwrite refusal; symlinked ancestry/source/destination plus content-drift refusal; and post-read Guide-race refusal before publication.
- The native Rust contract test passes pathless closed result and stable error mapping; Vitest passes 45 tests including strict pathless payload rejection.
- The full gate passes: 483 core tests (2 intentional ignores), 18 error-contract tests, 13 protocol tests, public-API/trybuild and unknown-YAML suites, 30 desktop Rust tests, 92 MCP tests, 3 approval-helper integration tests, 1 stdio integration test, strict Clippy, Cargo formatting, `pnpm check`, production Vite/Cargo build, Bootstrap validation (17 required files, 53 ADRs, 8 pinned agents), and whitespace checks.
- Existing Node 26.2.0 and installed pnpm 11.20.0 were used without installation or switching.
- Final Security Reviewer verdict: **PASS** with no blocking finding. The review confirmed closed/pathless IPC, session/revision binding, confined stable source verification, no-symlink destination retention, absent-only publication, explicit durability ambiguity, content-free errors, and no new process/MCP/logging authority.
