# Phase 3O — Native Tested Approval

- **Objective:** Let a user explicitly mark the exact current Guide content Tested through a native-only, revision-checked approval.
- **Required reasoning level:** xhigh for hash/trust design; high implementation; one xhigh security closure.
- **Prerequisites:** ADR 0015; accepted parser/binder, typed mutation, recovery update, retained Workspace capability, native dialog pattern.
- **Allowed files or subsystem:** ADR/content model, `rehow-core` domain/storage/application and tests, desktop Tauri/React/tests/styles, `PROGRESS.md`.
- **Relevant specifications and ADRs:** `docs/content-model.md` status/hash sections; ADRs 0010, 0011, 0014, 0015; `docs/threat-model.md`; `docs/testing-strategy.md`.
- **Exact required behavior:** Calculate frozen `verify:v1`; expose effective Tested only on a match; native command accepts only Guide ID/revision and invokes the core-owned native confirmation using its real parent window, safe title, and canonical ID. The private core mutation uses trusted time/hash. Draft/stale/mismatched-tested may become Tested; archived refuses; cancel/no-op preserves bytes.
- **Important states and edge cases:** Missing/malformed/archived Guide, stale revision, already effective Tested, externally forged/mismatched hash, unknown metadata, optional absent versus empty fields, negative zero, mapping order, unsafe round trip, native cancel, busy/recovery/refresh failure.
- **Security constraints:** No renderer/MCP status, timestamp, hash, path, or confirmation authority; no shell; exact revision; retained capability; content-free errors; spoof-resistant native text; atomic recoverable write.
- **Tests to add or update:** Cross-language hash vectors; semantic invariance/sensitivity; effective-status mismatch; transition matrix and exact emitted pair; conflict/atomic application tests; native confirmation/error tests; UI named action/busy/error/focus behavior.
- **Commands to run:** Focused Cargo/Vitest/type checks during work, then one full repository gate from `AGENTS.md`/testing strategy.
- **Required reviews and artifacts:** One Security Reviewer/xhigh closure and current web-interface-guideline static review for the final control.
- **Explicit non-goals:** User-supplied hashes, MCP approval, verification history, Attachment file-content hashing, native organizational trust preservation, migration, watcher, Git.
- **Definition of done:** Hash decision and vectors are frozen; forged/stale claims are not displayed Tested; only a confirmed native exact-revision operation can establish Tested; tests/full gate pass; signed feature/progress checkpoints exist.
- **Expected commit boundary:** One signed feature commit and one signed progress-only commit; Orchestrator owns both.
- **Conditions requiring escalation:** Hash ambiguity, need for caller-supplied trust data, unknown-metadata loss, non-atomic mutation, or inability to distinguish declared/effective status safely.
