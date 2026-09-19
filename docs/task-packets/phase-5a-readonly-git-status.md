# Phase 5A — confined read-only Git status

- **Objective:** Report bounded Git availability, branch, worktree counts, upstream divergence, conflicts, and remote names for the open Workspace without executing repository-selected programs or adding mutation/network authority.
- **Required reasoning level:** Architect/xhigh implementation ownership; Security Reviewer/xhigh checkpoint.
- **Prerequisites:** ADR 0006, ADR 0021, retained Workspace capability, accepted read-only Git surface.
- **Allowed files or subsystem:** `crates/rehow-core/src/git.rs`, narrow storage capability bridge, desktop Tauri adapter/UI/tests, ADR/evidence/progress docs.
- **Relevant specifications and ADRs:** Git sections of product/architecture/testing/threat model; ADRs 0003, 0006, 0012, 0021.
- **Exact required behavior:** Fixed absolute root-owned Git and Bubblewrap; direct argv; cleared/safe environment; an unshared network/mount/user namespace; descriptor-mounted read-only Workspace/object/ref/index facts in a synthetic config-free Git directory; reject gitfile/commondir and mask alternates; porcelain-v2 status; local no-include remote names only; bounded timeout/output; pathless IPC; compact accessible status.
- **Important states and edge cases:** Missing Git/repo, unsafe metadata, unborn/detached branch, clean/dirty/untracked/conflicted, ahead/behind/diverged, no/many/hostile remote names, nonzero/signal/timeout/oversize/invalid output.
- **Security constraints:** No shell, raw stderr, URLs, credentials, network, config includes, hooks/helpers, optional locks, arbitrary path, repository mutation, or Git mutation command.
- **Tests to add or update:** Temporary repositories; hostile hooks/fsmonitor/clean/process/include config; secret-bearing remote URL non-disclosure; external and post-check alternates; opaque non-UTF-8 porcelain paths; parser/payload/UI summaries; no sentinel execution.
- **Commands to run:** Focused core/native/Vitest tests, `pnpm check`, Cargo format and locked Clippy; full build/test only at checkpoint.
- **Explicit non-goals:** init/connect/clone/commit/sync/scheduling/authentication, remote URLs, file lists/diffs, branch mutation, external Git preference.
- **Definition of done:** Focused and checkpoint gates pass, one Security Reviewer has no blocker/high/medium finding, docs/progress match a signed commit.
- **Expected commit boundary:** One Phase 5A feature/security/docs commit, followed by the progress ledger.
- **Conditions requiring escalation:** Any repository-controlled execution, credential exposure, write, root escape, unbounded process behavior, or need for mutation/network authority.
