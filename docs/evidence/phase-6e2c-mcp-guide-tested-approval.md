# Phase 6E2C MCP Guide tested approval evidence

Date: 2026-09-21

`guide_tested_plan` validates an issue-free exact Guide revision through the core's existing unarchived/readiness/effective-status rules. Its private binding includes the portable Workspace identity, Guide ID/revision, Category ID, and resolved managed path. Read-only sessions still receive the bounded preview; an executable 90-second token requires startup write authority and the trusted sealed helper, not broad authority.

`guide_mark_tested` accepts only the launch handle and single-use token. The native helper uses the dedicated Tested consent language. Denial preserves the Guide byte-for-byte. Approval is followed by a fresh Workspace/identity/revision/path/Category/readiness check. The complete post-approval refresh and core mutation run in blocking work so the single-thread protocol runtime remains able to dispatch cancellation notifications. The core then reparses the exact bytes under the recovery writer lock, recalculates readiness and `verify:v1`, owns the trusted timestamps, and checks request cancellation immediately before journal execution.

Focused coverage proves the exact thirty-two-tool schema, absence of status/hash/time/path/consent inputs, approved transition and verification data, replay/scope/expiry refusal, already-Tested no-token preview, archived/unready refusal, read-only preview, write-without-broad execution availability, native denial byte identity, post-plan drift refusal, core commit-boundary cancellation, and deterministic real JSON-RPC cancellation paused immediately before journal creation with unchanged bytes and no journal. Existing cross-language verification vectors remain unchanged.

The reused Security Reviewer/xhigh first identified two material gaps: missing scope/expiry/archive refusal coverage and a blocked cancellation path caused by synchronous post-approval work on the current-thread protocol runtime. Both were corrected. The mutation now runs through blocking work, and a deterministic test pauses the real JSON-RPC operation at the final pre-journal cancellation check. The same reviewer then returned **PASS** with no remaining blocker, high, or medium finding.

The final gate passed on the repository state described here:

- `cargo test --workspace --all-targets --all-features --locked`: passed, including 449 core tests plus 2 intentional ignores, 68 MCP unit tests, 3 approval-helper integration tests, and 1 stdio integration test.
- strict workspace Clippy, Rust formatting, and locked workspace build: passed.
- Biome, both TypeScript checks, generated-contract verification, and cross-language Guide hash vectors: passed.
- desktop Vitest: 39 passed; production Vite build passed.
- Git SSH helper preparation, credential-proxy test, bootstrap validation, and `git diff --check`: passed. Bootstrap reports 17 required files, 44 ADRs, and 8 pinned project agents.

No new native dialog behavior was introduced: the already-tested sealed helper gained a closed Tested operation/consent kind, covered by unit and integration tests. The packet therefore requires no additional graphical smoke test for this phase.
