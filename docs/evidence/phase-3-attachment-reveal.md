# Phase 3 Attachment parent reveal evidence

Date: 2026-09-23

## Result

The desktop can show a verified Attachment's containing folder without exposing a path/URI to the renderer and without asking another application to open the Attachment. The request is bound to the Workspace session, Guide ID, exact Guide revision, and Attachment ID. Core verifies the stable confined source, 64 MiB cap, declared size, and SHA-256 before resolving the parent through retained no-follow directory descriptors.

The parent stays represented by a retained directory descriptor. GIO passes a duplicate of that descriptor to XDG Desktop Portal `OpenFile`; the core never reconstructs a pathname or URI for the native handoff. The descriptor remains bound to the verified directory across later leaf or intermediate pathname replacement. The async native command uses a single-flight blocking worker. There is no shell, caller-selected executable/argument, Attachment descriptor/URI, content handoff, mutation, or MCP surface.

## Interaction and design review

The project-pinned `web-design-guidelines` skill and its freshly fetched 2026-09-23 upstream rules were applied. `Show Folder` is a named semantic button with visible hover/focus, `Opening…` busy copy, mutual exclusion with other Attachment/Guide operations, strict stale-response suppression, polite success/error feedback, and actionable file-manager fallback guidance. The existing wrapped Attachment row remains narrow-safe, so no separate shell screenshot checkpoint was required.

## Verification

- Three focused core tests pass: exact verified parent preparation, linked-source/post-read Guide-race refusal, retained inode identity after leaf swaps, and intermediate-symlink refusal before handoff.
- Two focused native Rust tests pass closed/pathless payload, stable error mapping, and single-flight release; Vitest passes 46 tests including strict pathless response validation.
- The corrected full gate passes: 486 core tests (2 intentional ignores), 18 error-contract tests, 13 protocol tests, public-API/trybuild and unknown-YAML suites, 32 desktop Rust tests, 92 MCP tests, 3 approval-helper integration tests, 1 stdio integration test, 46 Vitest tests, strict Clippy, Cargo formatting, `pnpm check`, production Vite/Cargo build, Bootstrap validation (17 required files, 54 ADRs, 8 pinned agents), and whitespace checks.
- The already-locked GIO 0.18.4 edge adds no new package version; existing Node 26.2.0 and installed pnpm 11.20.0 were used without installation or switching.
- Final Security Reviewer correction verdict: PASS — no blocker, high, or medium findings. The review independently reran the three core and two native reveal tests.
