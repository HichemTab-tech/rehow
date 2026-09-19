# Phase 5B3C1 journaled clone content evidence

Date: 2026-09-19

## Boundary

- Only paths and exact sizes from the already accepted clone tree enter materialization.
- Staging bytes are re-read through owner-private no-follow descriptors and their Git blob IDs are independently recomputed before destination mutation.
- One `GitMaterialize` journal contains the complete target set, every target expects absence, directories are `0700`, and files are `0600`.
- The exact marker/scaffold and recovery journal bind destination identity, operation ID, Workspace ID, and tree plan hash. Unknown destination entries are preserved and refused.
- A partial scaffold resumes only when its exact operation marker remains; every existing level is preflighted before missing canonical directories are completed. Missing markers, foreign entries, links, special files, and wrong modes are refused without repair.
- The canonical scanner reopens the result and must match the validated Workspace identity and document revisions with no Attachment or schema issue. Exact descriptor-confined inventory and Git blob-ID checks cover every accepted file, including `.gitignore`, before success.
- This slice deliberately installs no Git objects, config, `HEAD`, or refs and is not exposed to the renderer.

## Focused evidence

- A manifest, `.gitignore`, Category descriptor, and Guide materialize through one journal and reopen as the expected Workspace.
- Same-size staging mutation is rejected by the independent Git object-ID check before destination mutation.
- A foreign destination entry is preserved; no managed content is published.
- A fault before the first live rename leaves the exact marker/journal and the same operation resumes to completion.
- Faults after marker publication and after each of the eight scaffold-directory boundaries resume to completion; deleting the marker or adding foreign residue is refused and never repaired or overwritten.
- Decoded `GitMaterialize` journals accept only `Absent -> Object(mode 0600)` targets.
- Journal limits now cover the complete accepted envelope of 21,002 files under a 32 MiB encoded bound; existing exact/one-over codec tests remain enforcing.

## Commands

Checkpoint command results are recorded in `PROGRESS.md`. No Node or pnpm installation or version switch is part of this slice.

- `cargo test -p rehow-core --all-features`: pass — 344 passed, 2 intentional ignores; 18 error-contract, 13 protocol, public API/trybuild, and 11 unknown-YAML tests also pass.
- `cargo clippy -p rehow-core --all-targets --all-features -- -D warnings`: pass.
- `cargo fmt --all --check`, `python3 scripts/check_bootstrap.py`, and `git diff --check`: pass.
- Reused Security Reviewer/xhigh correction review: PASS; no blocker/high/medium finding remains.
