# Task Packet: Phase 6E2A MCP Category reorder

- **Objective:** Expose preview-first, natively approved, exact-set Category reordering through the standalone MCP server.
- **Required reasoning level:** high implementation; xhigh security review.
- **Prerequisites:** Signed Phase 3H core reorder, Phase 6E1A capabilities, Phase 6E1B1 helper, Phase 6E1B2 sealed launcher/plans, ADRs 0026, 0039–0042.
- **Allowed files or subsystem:** `crates/rehow-core/src/protocol.rs`, `crates/rehow-mcp/`, `scripts/smoke-mcp-category-reorder.mjs`, Cargo lock/manifests, Phase 6 MCP/security/testing/development/installation docs, this packet/evidence/ADR, and `PROGRESS.md` after the feature commit.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/architecture.md`, `docs/threat-model.md`, ADR 0041, ADR 0042.
- **Exact required behavior:** Plan validates the exact complete Category snapshot/order and returns derived effects; executable tokens require write+broad authority and trusted native approval. Execute accepts only handle+token, consumes it, requests native approval, refreshes/rechecks all bound state, and invokes the existing atomic recoverable reorder.
- **Important states and edge cases:** Read-only/broad-disabled/helper-unavailable preview, empty and maximum inventory, duplicate/missing/unknown IDs, malformed snapshot/token, expiry, denial, replay, wrong Workspace, client cancellation, external edit, concurrent plan, no-op, busy, committed-refresh-required, and unsafe storage.
- **Security constraints:** No client approval assertion, raw path/order/revision/effect authority at execution, shell, arbitrary helper, credential access, or mutation before approval. Tokens are canonical, random, opaque, short-lived, exact-bound, and single-use. Refresh and recheck after approval.
- **Tests to add or update:** Exact 28-tool schema; direct and real-JSON-RPC approved reorder; replay; external-change refusal; capability and dry-run behavior; existing approval provenance/cancellation/adversarial suites.
- **Commands to run:** Focused MCP tests and strict Clippy while iterating; before commit run full Cargo tests/Clippy/format, pnpm check/test/build, bootstrap, and whitespace gates.
- **Required reviews and artifacts:** One reused Security Reviewer; real packaged graphical approve and deny smoke before acceptance/first release exposure.
- **Explicit non-goals:** Category removal, arbitrary order numbers, path changes, Category content edits, Guide moves, tested approval, Attachment mutation, Git writes, scheduled tasks, or remote MCP transport.
- **Definition of done:** Schemas are narrow; preview and approval-backed execution behave as specified; stale/replay/cancellation/capability tests pass; full gate passes; security review passes; docs/evidence/progress match the tree; signed checkpoints exist.
- **Expected commit boundary:** One signed implementation/docs checkpoint, followed by a signed progress-only checkpoint. The Orchestrator owns both.
- **Conditions requiring escalation:** Approval can be forged or replayed, exact state cannot be revalidated, helper packaging identity is absent in a production smoke, a destructive action becomes necessary, or the real GUI gate requires user interaction.
