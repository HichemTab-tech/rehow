# Phase 6C8 MCP Guide Tags/Aliases evidence

Date: 2026-09-20

The exact twenty-tool surface adds separate `guide_set_tags` and `guide_set_aliases` operations. Each requires the existing per-alias write grant, Guide ID, exact revision, and one complete ordered replacement array. The MCP adapter delegates to typed core mutations and the existing recovery journal; it cannot submit unrelated Guide fields.

Focused tests cover capability-before-parse denial without recovery-state creation, exact schema exclusions, successful replacement and clearing, no-op revision stability, stale-revision refusal, Unicode-fold duplicate refusal with byte identity, tested-to-stale behavior, path/title/summary/body/Category/other-collection/unknown-metadata preservation, and real JSON-RPC execution. Existing recovery, unsafe-storage, external-write, and confinement suites remain shared evidence.

Independent Security Reviewer result: PASS, with no findings.
