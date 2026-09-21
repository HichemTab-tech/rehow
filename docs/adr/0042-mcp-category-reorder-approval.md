# ADR 0042: MCP Category reorder approval

- Status: Accepted for Phase 6E2A
- Date: 2026-09-20

## Decision

`category_reorder_plan` is a read-only preview that accepts one startup Workspace handle, the exact Category-inventory snapshot, and the complete ordered Category-ID set. Rust resolves every current descriptor revision, rejects malformed, duplicate, missing, incomplete, or issue-bearing inventories, derives canonical `1000`-spaced order values, and returns the exact targets, an informational effect fingerprint, warnings, and—only when write+broad startup authority and the trusted helper are present—a random 90-second plan token.

The private plan binds the launch handle, portable Workspace ID, expected snapshot, complete ordered ID/revision effect set, operation kind, and native-dialog summary. `category_reorder` accepts only the Workspace handle and canonical token. It consumes the token, obtains approval only through the sealed native helper, then freshly refreshes and rechecks the complete snapshot, every Category revision, and issue-free inventory before calling the existing recoverable multi-document reorder operation. The core repeats the complete snapshot check after acquiring the recovery writer lock and again after candidate/object preparation immediately before journal execution, with cancellation checks bracketing that final scan. Approval denial is a successful no-change result; malformed, expired, replayed, cross-Workspace, stale, busy, cancelled, and unsafe states are structured failures. The handler extracts the SDK request-cancellation token, races it against the blocking native helper, kills/reaps the helper through the approval manager, and cannot leave reusable execution authority.

## Consequences

- MCP cannot submit order values, revisions, paths, effect text, confirmation fields, or a consent result at execution time.
- Preview remains available without mutation capability, while executable tokens exist only for aliases explicitly granted both write and broad authority.
- Every execution attempt is single-use. Concurrent or external changes after planning fail before mutation even after user approval.
- Category paths, Guides, status, names, summaries, and unknown metadata remain outside this operation.
- The first production release exposing this tool must package the digest-bound sibling and pass a real graphical approve/deny smoke test.
