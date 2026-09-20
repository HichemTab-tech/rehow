# Phase 6C5 MCP Category-lifecycle evidence

Date: 2026-09-20

The exact fifteen-tool surface adds separate `category_archive` and `category_restore` operations. Each requires the existing per-alias write grant plus Category ID and exact descriptor revision; neither accepts a status value. The typed core mutation preserves directory, Guides, order, ID, creation time, and unknown metadata.

All Category descriptor updates now recheck a complete inventory snapshot through fresh descriptor-confined inspection under the recovery writer lock. Because the snapshot includes the manifest revision, an external change that makes the Category default conflicts before archive and leaves the descriptor active. The snapshot is included in the journal plan.

Focused evidence covers schemas, read-only denial, archive/restore round trip, directory/Guide preservation, default refusal, external default change under the lock, Category-details regression, and real JSON-RPC archive execution. Existing core lifecycle, recovery, confinement, presentation, and unknown-metadata suites remain shared evidence.

Inventory-only conflicts return correlated `SNAPSHOT_CONFLICT`; changed target descriptors retain the more specific `REVISION_CONFLICT`. Independent Security Reviewer result: PASS, with no findings.
