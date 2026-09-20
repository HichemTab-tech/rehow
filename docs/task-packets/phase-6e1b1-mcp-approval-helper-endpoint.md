# Task Packet: Phase 6E1B1 MCP approval helper endpoint

- **Objective:** Package and adversarially test the inert native approval helper endpoint without connecting it to MCP.
- **Required reasoning level:** xhigh security boundary; one Security Reviewer required.
- **Prerequisites:** signed Phase 6E1A checkpoint `82da5d0`; ADRs 0026, 0039, and 0040.
- **Allowed files or subsystem:** `crates/rehow-mcp` helper binary/protocol/tests, lockfile dependency edge, and directly applicable docs.
- **Relevant specifications and ADRs:** `docs/mcp-spec.md`, `docs/threat-model.md`, ADRs 0039 and 0040.
- **Exact required behavior:** Fixed descriptors 3/4; exact `RHA1` frame; bounded plain text; fixed dialog chrome; one-byte Yes/No response; silent fail-closed headless/error behavior.
- **Important states and edge cases:** missing/wrong-direction/non-pipe descriptors; partial/stalled/oversized/trailing frames; invalid UTF-8/control/bidi/line bounds; headless session; output failure; hostile inherited descriptors.
- **Security constraints:** No argv/PATH/subprocess/stdin/stdout/stderr/filesystem/public socket/token/secret/effect channel; use only the in-process GTK dialog; mark approval pipes close-on-exec and close all unrelated descriptors; no caller path, Workspace, mutation, Git, network, or MCP authority.
- **Tests to add or update:** parser boundary/adversarial unit tests including visual separators; real helper missing-pipe/headless execution; hostile-`PATH` fake Zenity non-execution; `/proc/<pid>/fd` proof of only 0–4 and `/dev/null` standard streams.
- **Commands to run:** focused helper and MCP tests; pnpm check/test/build; strict Workspace Clippy; Cargo formatting; bootstrap/whitespace checks.
- **Required reviews and artifacts:** one independent Security Reviewer and `docs/evidence/phase-6e1b1-mcp-approval-helper-endpoint.md`.
- **Explicit non-goals:** server-side sibling capture/digest/exec, plan storage, approval request tools, any mutation, automated GUI clicks.
- **Definition of done:** malformed/headless executions cannot approve or emit protocol output; descriptor isolation is proven; full gates/review/docs pass; helper remains unreachable.
- **Expected commit boundary:** helper/protocol/tests/dependency edge/ADR/packet/evidence/aligned docs; progress follows separately.
- **Conditions requiring escalation:** inability to close descriptors, need for an effect channel outside the pipes, GUI framework version change, or any route from MCP to the helper/mutation.
