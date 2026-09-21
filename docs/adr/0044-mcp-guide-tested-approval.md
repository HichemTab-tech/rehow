# ADR 0044: MCP Guide tested approval

- Status: Accepted for Phase 6E2C
- Date: 2026-09-21

## Decision

`guide_tested_plan` is a read-only preview for one exact Guide ID and revision. The core validates that the current Guide is unarchived, structurally ready for Tested status, and not already effectively Tested. The preview returns only the canonical ID, safe title, revision, current status, exact effects, warnings, and an informational effect fingerprint. It issues an executable 90-second token only when the Workspace has startup write authority and the sealed native approval helper is trusted; broad authority is not required.

The private plan binds the launch handle, portable Workspace ID, Guide ID/revision, resolved managed path and Category identity, tested operation kind, exact effects, and native-dialog summary. `guide_mark_tested` accepts only the Workspace handle and plan token, consumes the token, and accepts consent only from the sealed native helper using the dedicated Tested approval language. Denial is a successful no-change result.

After approval, the server refreshes and rechecks the issue-free Workspace, exact Guide identity/revision/path/Category, unarchived state, and readiness. The core then reparses the exact revision under the recovery writer lock, revalidates readiness, calculates `verify:v1`, and atomically writes Tested status, verification hash, and trusted timestamps. A cancellation callback is checked at the core journal boundary; once journal execution begins, recovery semantics own completion.

ADR 0015's prohibition applies to MCP client assertions and generic/headless mutation APIs. This phase permits one explicitly named core application entry point for trusted adapters after native confirmation. It accepts no approval Boolean, status, hash, timestamp, path, or client effect data. Desktop retains its direct real-parent-window confirmation entry point.

## Consequences

- An MCP client cannot mark a Guide Tested by submitting `status`, a confirmation flag/phrase, a timestamp, a hash, or a helper result.
- AI-created or AI-edited content stays Draft/Stale until a human approves that exact revision in the native dialog.
- Plan expiry, replay, denial, request cancellation, cross-Workspace use, malformed/archived/unready content, duplicate identity, and external drift fail without mutation.
- Existing verification vectors, effective-Stale behavior, recovery, unknown-metadata preservation, and subsequent edit invalidation remain authoritative.
- This phase adds no Attachment, delete, broad, Git, path, shell, or remote-server authority.
