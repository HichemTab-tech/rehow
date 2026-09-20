# Task Packet: Phase 6D2 MCP Attachment and Git status reads

- **Objective:** Add dedicated bounded Attachment metadata listing and confined Git status in one read-only checkpoint.
- **Required reasoning level:** high locally; no implementation delegation.
- **Prerequisites:** signed Phase 6D1 checkpoint `787176c`, ADRs 0021, 0027, and 0038.
- **Allowed files or subsystem:** MCP server/tests and directly applicable docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/content-model.md`, `docs/threat-model.md`, ADRs 0021, 0027, and 0038.
- **Exact required behavior:** List one Guide's declared Attachment metadata/revision; return closed safe Git availability/repository/branch/divergence/change/remote-name state through the existing core reader.
- **Important states and edge cases:** invalid/missing Guide; empty/maximum Attachment collection; Git missing, non-repository, unsafe, ready/clean/dirty/conflicted/detached, no upstream, unknown divergence, bounded failure.
- **Security constraints:** No caller or host path, Attachment bytes, URL, raw Git output, generic arguments, environment, credentials, network, mutation, broad, destructive, or tested authority.
- **Tests to add or update:** exact twenty-six-tool schema; real Attachment metadata; isolated Git repository with remote URL non-disclosure and untracked count; real protocol routing; full gates.
- **Commands to run:** focused MCP tests; `pnpm check/test/build`; strict Workspace Clippy; Cargo format; bootstrap and whitespace checks.
- **Explicit non-goals:** Attachment transfer/mutation, Git commit/sync, recovery execution, network/authentication, UI.
- **Definition of done:** both outputs are bounded/pathless/closed, tests and full gate pass, and docs match code.
- **Expected commit boundary:** tools/tests plus ADR/packet/evidence and aligned docs; progress follows separately.
- **Conditions requiring escalation:** raw Git/URL/path/credential disclosure or any new process/network/write authority outside the accepted core reader.
