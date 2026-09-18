# Phase 1D-C2a required-field mutation evidence

Date: 2026-09-18 (Africa/Algiers)

Status: implementation checkpoint complete. Optional fields, collections, and Guide archive/restore remain in C2b; the combined Phase 1D-C review remains pending until C2b is complete.

The private `domain::content::mutation` module now plans in-memory, revalidated candidates for Workspace name changes; Category name, order, archive, and restore; and Guide title, summary, Category, body, and stale transitions. It performs no filesystem I/O and exposes no public mutation API.

Evidence:

- invalid payloads are rejected before no-op detection, while a valid no-op returns exact source bytes without requiring a writable round-trip artifact;
- actual edits require proven source reconciliation, use typed canonical replacements, reject overlapping or unsafe spans, and cap each plan at eight edits;
- untouched bytes, unknown metadata, comments outside edited values, and Guide frontmatter/body boundaries are preserved;
- every candidate is reparsed and rebound through the production parser and binder, then checked against intended values and the untouched known/unknown state;
- `updated_at` is monotonic (`max(current, trusted stamp)`); actual edits stale tested Guides and preserve that effective stale state through archived status;
- explicit stale transitions cover draft, tested, stale, and archived prior-state cases without allowing mutation code to create `tested`;
- errors are typed and content-free, allocation and size failures are handled, and no filesystem, shell, network, Git, Tauri, MCP, clock, entropy, or logging authority is present;
- a trybuild case proves the mutation planner is not reachable through the crate's public API.

Verified locally with formatting, warnings-denied all-feature Clippy, the full all-feature `rehow-core` suite (175 library, 18 error-contract, 13 protocol, 1 public-API/31 compile-fail, and 11 unknown-YAML tests), and documentation tests.
