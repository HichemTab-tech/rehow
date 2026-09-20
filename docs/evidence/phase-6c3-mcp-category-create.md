# Phase 6C3 MCP Category-create evidence

Date: 2026-09-20

The exact twelve-tool surface adds only `category_create`. `categories_list` now returns an opaque snapshot of the whole valid Category inventory. Creation requires that snapshot plus the existing per-alias write grant; callers provide only name and optional summary. Rehow generates the Category ID, timestamps, path, order, and active status.

The core rechecks the raw Category entry limit, fresh compatible Workspace state, and complete inventory snapshot while holding the cooperating recovery writer lock. The snapshot and target are bound into the journal plan. External descriptor changes and stale views at the 1,000-entry cap write no Category; committed creates whose refresh fails are reported as committed with `refreshRequired`.

Focused evidence covers the exact schema, read-only denial, canonical creation, opaque snapshot return, an external Category edit before locked creation, and stale-view raw-entry-limit enforcement. Existing recovery interruption, confinement, canonical emission, and resource-limit suites remain shared evidence.

Independent Security Reviewer result: PASS, with no findings.
