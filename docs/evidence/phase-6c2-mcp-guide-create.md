# Phase 6C2 MCP Guide-create evidence

Date: 2026-09-20

The exact eleven-tool surface adds only `guide_create`. It is absent as write authority unless the configured alias separately receives `--allow-write`. Its schema accepts only a launch handle, Category ID, exact Category descriptor revision, title, and summary. Rehow generates the Guide ID, timestamps, canonical filename/path, draft status, empty Tags/aliases, and starter body.

The shared core now rechecks the Category descriptor bytes/revision and descriptor-confined global Guide-candidate count while holding the cooperating recovery writer lock, before testing the generated Guide target for absence. The prerequisite path/revision are bound into the recovery plan hash. This closes retained-view races for both MCP and existing native Guide creation. A real external Category edit produces a correlated `REVISION_CONFLICT` and no extra Markdown file; a stale view whose disk state has reached 10,000 Guide candidates produces `RESOURCE_LIMIT` and no write.

Focused evidence covers exact tool/schema shape; absent caller ID/path/status/time/body fields; read-only denial without recovery-state creation; successful canonical draft creation and reread; core-generated fields; a deterministic external Category mutation between retained view and locked create; and a stale-view on-disk hard-cap regression. Existing core emission, absent-only publication, journal interruption, path confinement, count/size, and refresh suites remain shared evidence.
