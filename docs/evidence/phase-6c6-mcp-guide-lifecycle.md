# Phase 6C6 MCP Guide-lifecycle evidence

Date: 2026-09-20

The exact seventeen-tool surface adds separate `guide_archive` and `guide_restore` operations. Each requires the existing per-alias write grant plus Guide ID and exact revision; neither accepts a generic status or content field. The existing typed core lifecycle and recovery path preserves the Markdown path, body, Category, Attachments, and unknown metadata.

Focused tests cover exact schemas, read-only denial without recovery-state creation, archive/restore round trip, stable path/body/Category, declared/effective status output, and real JSON-RPC archive execution. Existing core stale-revision, external-write, tested/stale lifecycle, presentation, recovery, and confinement suites remain shared evidence.

Independent Security Reviewer result: PASS, with no findings.
