# Phase 5B1 — initialize Git for the open Workspace

- **Objective:** Create a valid direct Git repository for the currently open compatible Workspace after a native-confirmed, revision-bound plan, without executing Git or modifying Workspace content.
- **Required reasoning level:** Architect/xhigh implementation ownership; Security Reviewer/xhigh checkpoint.
- **Prerequisites:** ADRs 0006, 0021, and 0022; retained Workspace capability; Phase 5A status.
- **Allowed files or subsystem:** Core Git/storage/protocol bridge, desktop Tauri adapter/UI/tests, ADR/evidence/progress and narrow architecture/testing documentation.
- **Relevant specifications and ADRs:** Git sections of product/architecture/testing/threat model; ADRs 0003, 0006, 0009, 0012, 0021, 0022.
- **Exact required behavior:** Pathless plan with random single-use token, Workspace ID/revision/root binding, fixed `main`; apply rechecks state and requires a native dialog; core creates only canonical private `.git` metadata through retained descriptors; interrupted canonical work resumes via an exact marker; status refreshes after success.
- **Important states and edge cases:** Read-only Workspace, existing/symlink/file/unsafe `.git`, stale/replayed/substituted plan, changed manifest/root, empty partial directory, canonical marked partial state, unknown residue, file/directory collision, unsupported durability, cancellation, post-commit status failure.
- **Security constraints:** No caller path, arbitrary branch, shell, Git subprocess, config/template/hook/helper, credential, network, checkout, staging, commit, deletion of unknown bytes, or overwrite. All created entries are owner-private, no-follow, single-link where regular, flushed, and root-confined.
- **Tests to add or update:** Temporary Workspace success and Git recognition; exact modes/content; stale/replay/token mismatch; symlink/file/residue refusal; deterministic interruption/resume points; pathless bounded IPC; UI confirmation/result states.
- **Commands to run:** Focused core/native/Vitest tests; formatting, `pnpm check`, locked Clippy; full test/build/Tauri gate once at acceptance.
- **Required reviews and artifacts:** One Security Reviewer/xhigh after focused gates; adversarial evidence document. No separate design reviewer unless shell geometry or interaction model materially changes.
- **Explicit non-goals:** Remote config, clone, credentials, fetch/pull/push, staging, commit, branch switching, repair/deletion of noncanonical repositories.
- **Definition of done:** Focused and full checkpoint gates pass; one Security Reviewer has no blocker/high/medium finding; docs/progress match a signed commit.
- **Expected commit boundary:** One Phase 5B1 feature/security/docs commit followed by the progress ledger.
- **Conditions requiring escalation:** Need to delete/overwrite unknown `.git` data, execute repository-selected code, accept a caller path, weaken durability/confinement, or add config/network/credential authority.
