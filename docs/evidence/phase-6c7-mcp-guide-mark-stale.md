# Phase 6C7 MCP Guide mark-stale evidence

Date: 2026-09-20

The exact eighteen-tool surface adds `guide_mark_stale`. It requires the existing per-alias write grant, Guide ID, and exact revision, and accepts no generic status, reason, content, path, Category, or tested field. The existing typed core mutation and recovery path owns the complete state transition.

Focused tests cover capability-before-parse denial without recovery-state creation, exact schema exclusions, stale-revision refusal, draft/tested/stale/archived transitions, archived restorable-state output, no-op revision stability, path/body/Category preservation, and real JSON-RPC execution. Existing core recovery, unknown-metadata, unsafe-storage, external-write, and confinement suites remain shared evidence.

Independent Security Reviewer result: PASS, with no findings.
