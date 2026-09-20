# Task Packet: Phase 6D1 MCP Guide validation and discovery

- **Objective:** Add the three remaining read-only Guide intelligence tools in one bounded checkpoint.
- **Required reasoning level:** high locally; no delegation required.
- **Prerequisites:** signed Phase 6C9 checkpoint `45505e3`, ADRs 0027 and 0037.
- **Allowed files or subsystem:** core search normalization API, MCP server/tests, and directly applicable docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, ADRs 0027 and 0037.
- **Exact required behavior:** Validate one loaded Guide without paths; find conservative exact identity duplicates; rank related Guides with closed reasons; authenticate pagination to the Workspace view and query.
- **Important states and edge cases:** unknown/invalid Guide IDs, missing related IDs, Attachment/verification diagnostics, one-way relationships, archived Guides, Unicode normalization, pair duplication, global result cap, stale/tampered cursors.
- **Security constraints:** Read-only; no paths, bodies, Markdown interpretation, mutation, Git, shell, network, broad, destructive, or tested authority.
- **Tests to add or update:** exact twenty-four-tool schema, validation warning semantics, duplicate score/reasons, related ranking/pagination, real protocol routing, full gates.
- **Commands to run:** focused MCP tests; `pnpm check/test/build`; strict Workspace Clippy; Cargo format; bootstrap and whitespace checks.
- **Explicit non-goals:** fuzzy/embedding similarity, automatic merge, mutation, external refresh, Attachment bytes, Git tools.
- **Definition of done:** outputs are bounded/pathless/explainable, tests and full gate pass, and docs match code.
- **Expected commit boundary:** tool schemas/logic/tests plus ADR/packet/evidence and aligned docs; progress follows separately.
- **Conditions requiring escalation:** unbounded pair generation, content/path disclosure, or new write/process/network authority.
