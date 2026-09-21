# ADR 0043: MCP Category removal approval

- Status: Accepted for Phase 6E2B
- Date: 2026-09-21

## Decision

`category_remove_plan` is a read-only preview for one exact Category ID, descriptor revision, and complete Category-inventory snapshot. It issues an executable 90-second token only when the Category is empty, is not the Workspace default, the Workspace is issue-free, startup write+broad authority exists, and the trusted native helper is available.

The private plan binds the launch handle, portable Workspace ID, snapshot, Category ID/revision, resolved relative directory, destructive operation kind, exact effects, and native-dialog summary. `category_remove` accepts only the Workspace handle and token, consumes the token, and accepts consent only from the sealed native helper. After approval it refreshes and rechecks the snapshot, revision, empty state, default state, and issues. The core repeats the complete snapshot check under the recovery writer lock and again immediately before journal execution, with cancellation checks around both scans.

Execution reuses the accepted recovery operation: the exact descriptor moves to `.rehow/trash/<operation>/...`, then only the verified-empty Category directory is removed nonrecursively. Removal never cascades to Guides or Attachments. Default-Category replacement is not accepted by this phase; reassignment and removal would require a separately specified atomic multi-target operation.

## Consequences

- MCP cannot supply a path, effect, approval assertion, replacement default, or deletion target at execution time.
- Preview remains read-only; executable plans require broad startup authority and explicit native destructive approval.
- Denial changes nothing. Expiry, replay, cancellation, cross-Workspace use, external drift, nonempty/default state, malformed content, and unsafe storage fail closed. If an uncooperative writer creates content in the final post-scan race window after the descriptor has moved, Rehow preserves that content and the trashed descriptor and returns `RECOVERY_REQUIRED` with the exact operation and affected path rather than misreporting a no-write stale plan.
- The operation is recoverable but intentionally destructive from the user's perspective, so it receives a dedicated graphical approve/deny smoke before acceptance.
