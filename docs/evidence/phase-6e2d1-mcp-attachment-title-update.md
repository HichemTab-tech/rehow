# Phase 6E2D1 MCP Attachment title update evidence

Date: 2026-09-21

`attachment_metadata_update` is a write-capability-only exact-revision mutation. Its closed input has the Workspace handle, Guide ID/revision, Attachment ID, and replacement display title. Schema tests prove it has no path, source path, bytes, hash, size, media type, status, or consent field.

The typed core planner locates the existing Attachment by stable ID, validates its title, edits only the existing title scalar in a proven block mapping, preserves unknown root and nested Attachment metadata, and rebinds the candidate before publication. Path, ID, media type, SHA-256, optional size, other Attachments, body, and unrelated Guide metadata remain identical. Tested trust becomes Stale; an identical title is byte-identical.

Focused core mutation, MCP write-gate/success/no-op, exact-tool-schema tests, formatting, and strict affected-crate Clippy pass. The full repository gate also passes: 450 core tests plus 2 intentional ignores, 25 desktop Rust tests, 69 MCP unit tests, 3 approval-helper integration tests, 1 stdio integration test, supporting contract/API suites, strict workspace Clippy/format/build, Biome, both TypeScript checks, contract/hash vectors, 39 Vitest tests, Git helper/credential smoke, Vite production build, bootstrap validation, and whitespace checks. Bootstrap reports 17 required files, 45 ADRs, and 8 pinned project agents.

The independent security review is intentionally batched with the later destructive Attachment removal slice. This checkpoint therefore proves implementation and tests but does not yet claim the complete Attachment mutation batch is accepted.
