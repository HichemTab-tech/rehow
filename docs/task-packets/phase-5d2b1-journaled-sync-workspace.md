# Task Packet: Journaled remote-fast-forward Workspace materialization

- **Objective:** Materialize the portable Workspace snapshot from a proven remote fast-forward through the existing recovery journal without publishing Git metadata.
- **Required reasoning level:** xhigh because this changes confined filesystem and recovery behavior.
- **Prerequisites:** Signed Phase 5D2A checkpoint `95ec8aa`; ADR 0025; opaque validated remote tree and ancestry authority.
- **Allowed files or subsystem:** `crates/rehow-core/src/git.rs`, `storage.rs`, `storage/recovery.rs`, `storage/recovery/workspace.rs`, ADR/testing/implementation evidence, and progress. No adapters, dependencies, or product UI.
- **Relevant specifications and ADRs:** ADRs 0009, 0023–0025; Git/recovery sections of architecture, threat model, and testing strategy.
- **Exact required behavior:** Accept only a clean proven remote fast-forward; revalidate the local plan and staged tree; compute the sorted current/incoming managed inventories; journal changed, created, and removed portable files atomically; move removals to operation-bound trash; preserve file modes on replacement; reopen and require exact remote tree equality; leave active Git objects, index, refs, config, and reflogs unchanged.
- **Important states and edge cases:** Unchanged files, `.gitignore`, new/removed Category directories, Guide replacement/removal, Attachment create/replace/remove, stale revisions, unsafe directory ownership/modes, symlinks, interrupted journal replay, malformed remote content, resource limits, and local/staging drift.
- **Security constraints:** Retained Workspace capability only; no arbitrary path or shell; no symlink following; owner-only/non-writable directory checks; bounded files/inventory; exact revision/blob validation; durable before-images; operation/path/inode-bound ownership for newly published directories; nonrecursive identity-safe empty-directory quarantine; redacted opaque authority.
- **Tests to add or update:** End-to-end update/create/trash/Category cleanup with unchanged `HEAD`; Category and Attachment replacement races; failed new-Category and Attachment-root cleanup; pre-existing empty Attachment preservation; existing recovery suites; focused sync tests; strict Clippy; full Workspace tests.
- **Commands to run:** `cargo fmt --all --check`; strict workspace Clippy; `cargo test --workspace --all-features`; bootstrap and whitespace checks.
- **Required reviews and artifacts:** One reused Security Reviewer at the completed filesystem-authority gate and focused evidence.
- **Explicit non-goals:** Active object/index/ref publication, complete crash-atomic sync transaction, merge/rebase, push, credentials, scheduling, native/UI, MCP, or cleanup of retained staging.
- **Definition of done:** The internal primitive is confined, bounded, recoverable, exact-tree verified, test-green, and unavailable to adapters until a durable sync intent composes Workspace and Git publication.
- **Expected commit boundary:** One core/docs checkpoint, followed by the progress ledger.
- **Conditions requiring escalation:** Any need to publish refs before exact Workspace recovery, weaken confinement, infer conflict resolution, expose partial authority, or mutate unrelated repository state.
