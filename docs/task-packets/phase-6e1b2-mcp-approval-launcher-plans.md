# Task Packet: Phase 6E1B2 MCP approval launcher and plans

- **Objective:** Add the verified server-side helper launcher and bounded in-memory approval-plan state without exposing an MCP operation.
- **Required reasoning level:** xhigh security boundary; one Security Reviewer at the completed gate.
- **Prerequisites:** signed Phase 6E1B1 checkpoint `b4eb876`; ADRs 0026, 0039, 0040, and 0041.
- **Allowed files or subsystem:** `crates/rehow-mcp` approval/launcher/state/tests, Cargo lock only if necessary, and directly applicable docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/threat-model.md`, ADRs 0040–0041.
- **Exact required behavior:** Capture only a digest-matching safe sibling into a sealed immutable executable snapshot; execute its held descriptor with exact private pipes and minimal environment; accept only exact successful one-byte approval; issue at most 64 random single-use plans with closed kinds, exact binding, 90-second maximum lifetime, and cancellation.
- **Important states and edge cases:** missing/malformed digest; missing/symlink/multi-link/writable/non-executable/oversized/raced helper; sibling replacement; pipe/fork/exec failure; zero/nonzero/signal/trailing/no output; expiry before/during launch; pending/active cancellation; replay; wrong Workspace/operation/binding; capacity exhaustion.
- **Security constraints:** No caller path, `PATH`, shell, argv effect, inherited loader/module/credential environment, public channel, filesystem plan, arbitrary process, approval token disclosure, Workspace mutation, or MCP tool. Close all unrelated child descriptors and reap every child.
- **Tests to add or update:** capture rejection corpus; held-snapshot path-swap and same-inode-overwrite execution fixtures; headless real-helper denial; exact result/exit rules; process-group expiry/cancellation; finalization-versus-cancel race; plan binding/replay/cap/expiry/cancellation; no tool-schema change.
- **Commands to run:** focused MCP tests; `pnpm check`, `pnpm test`, `pnpm build`; strict all-Workspace Clippy; Cargo formatting; bootstrap/whitespace checks.
- **Required reviews and artifacts:** one reused Security Reviewer and `docs/evidence/phase-6e1b2-mcp-approval-launcher-plans.md`.
- **Explicit non-goals:** any public approval request/cancel tool, any mutation, server persistence, GUI automation, or Phase 7 package automation.
- **Definition of done:** only the verified held helper can run; every non-exact or expired outcome denies and is reaped; plans are bounded/bound/single-use/cancellable; full gates/review/docs pass; MCP schema remains unchanged.
- **Expected commit boundary:** launcher/plan state/tests/ADR/packet/evidence/aligned docs; progress follows separately.
- **Conditions requiring escalation:** inability to use held-descriptor execution, need to inherit unsafe environment or expose a consent channel, non-reapable child behavior, or any mutation/tool authority.
