# Task Packet: Phase 6E2D2 MCP Attachment removal

- **Objective:** Expose one exact Attachment removal through read-only planning, sealed native approval, and recoverable core execution.
- **Required reasoning level:** high implementation; xhigh combined Attachment security review.
- **Prerequisites:** Accepted ADRs 0009, 0017, 0040–0041, and 0046; accepted native Attachment removal and MCP approval launcher.
- **Allowed files or subsystem:** Core Attachment removal application/storage cancellation seam; MCP server/approval protocol; Phase 6 docs/evidence; progress after feature commit.
- **Relevant specifications and ADRs:** `docs/content-model.md` Attachments/revisions, `docs/mcp-spec.md`, `docs/threat-model.md`, ADRs 0017 and 0046.
- **Exact required behavior:** Read-only preview for one Guide revision/Attachment ID; token only with write+broad/trusted-helper/recovery readiness; handle+token-only execution; destructive native consent; exact revalidation; bounded before-image streaming; metadata-first recoverable trash; single use; denial is no-op.
- **Important states and edge cases:** Read-only/no-broad/helper unavailable, missing/archived/stale Guide, missing/drifted/symlink/hardlink/special/oversized Attachment, denial, replay, expiry, wrong Workspace, cancellation during helper and immediately before journal, busy, refresh-required, interrupted recovery.
- **Security constraints:** No client path/bytes/hash/size/mode/effect/operation/consent; strict Workspace confinement; no shell/network/credentials; no permanent or recursive delete; sanitized native summary; exact preview binding; unknown metadata preservation.
- **Tests to add or update:** Exact 35-tool schema; preview capability matrix; apply/replay/denial; scope/expiry/archive/missing/drift; recoverable trash bytes; Guide metadata preservation/trust reduction; real JSON-RPC cancellation at the final pre-journal boundary with byte identity and no journal; strict Clippy/full gate.
- **Commands to run:** Focused core/MCP tests and strict Clippy while iterating; one full repository gate before checkpoint.
- **Required reviews and artifacts:** One Security Reviewer covering both 6E2D1 title update and 6E2D2 removal after focused tests pass; evidence document. Reuse accepted helper packaging; no new graphical smoke unless helper behavior changes.
- **Explicit non-goals:** Metadata add, raw transfer, host paths, bulk removal, permanent deletion, directory cleanup, rename, rendering/export/reveal, Git tools.
- **Definition of done:** Narrow schemas, exact native approval binding, recoverable execution, adversarial/cancellation tests, full gates, combined security PASS, truthful evidence/progress, signed checkpoints.
- **Expected commit boundary:** One implementation/docs checkpoint after review and gates, followed by progress-only ledger if useful.
- **Conditions requiring escalation:** Exact file effect cannot be bound, cancellation can begin a journal after observation, recovery would permanently delete, confinement weakens, or a genuine human checkpoint occurs.
