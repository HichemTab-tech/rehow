# Phase 6C9 MCP Guide Category-change evidence

Date: 2026-09-20

The exact twenty-one-tool surface adds `guide_change_category`. It requires the existing per-alias write grant plus exact Guide and destination Category revisions. The core derives source/destination paths, preserves the filename, verifies the destination descriptor under the recovery writer lock, and reuses the accepted destination-first journaled move.

Focused tests cover capability-before-parse denial without recovery-state creation, exact schema exclusions, lock-verified same-Category no-op, successful pathless move, stable ID/filename/title/summary/Tags/aliases/body/unknown metadata, tested-to-stale behavior, stale Guide refusal, concurrent destination archive correlated as a Category revision conflict, current archived-destination refusal, and real JSON-RPC execution. Existing destination-collision, interruption/replay, symlink/hardlink, unsafe-storage, and confinement suites remain shared evidence.

Independent Security Reviewer result: PASS, with no findings.
