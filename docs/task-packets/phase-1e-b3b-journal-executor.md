# Task Packet: Journal-driven Workspace executor

- **Objective:** Bind the durable journal/store to atomic managed-file creation and replacement, including deterministic resume after interruption.
- **Required reasoning level:** xhigh security/recovery boundary.
- **Prerequisites:** ADR 0009 and checkpoints `e810658`, `f26a868`, `dede741`.
- **Allowed files or subsystem:** `rehow-core::storage::recovery::workspace`, the retained root identity helper, this packet, and progress.
- **Relevant specifications and ADRs:** architecture Atomic writes, threat model concurrent/partial writes, ADRs 0003/0009.
- **Exact required behavior:** preflight root identity and all referenced objects; install generation one before mutation; apply object create/update targets in order; persist whole generations through applying, swapped, directory-synced, complete, and committed; resume a target already durably exchanged; record conflict without overwriting a neither-state.
- **Important states and edge cases:** new operation, committed replay, post-exchange/pre-journal crash, stale expected revision, neither-state, missing/corrupt object, root replacement, generation overflow, and multi-target ordered progress.
- **Security constraints:** lock required by store calls; exact bytes/mode/revision; managed paths only; no deletion/trash yet; no guessed recovery or unsafe rollback; preserve displaced bytes and content-free errors.
- **Tests to add or update:** complete journal execution, replay after physical exchange with prepared journal, external neither-state conflict, and existing atomic-target adversarial cases.
- **Commands to run:** format; focused recovery tests; full core tests at the combined recovery checkpoint; core Clippy; contract/diff checks.
- **Required reviews and artifacts:** one combined Security Reviewer after the recovery checkpoint, subject to available quota.
- **Explicit non-goals:** delete/trash execution, temp pruning, portable backups, adapter exposure, Git, MCP.
- **Definition of done:** create/update journals either durably commit, resume idempotently, or persist a conflict without overwriting unknown content.
- **Expected commit boundary:** journal executor/resume tests and progress update.
- **Conditions requiring escalation:** recovery requires guessing, an accepted object is unavailable/corrupt, or root/target identity is unsafe.
