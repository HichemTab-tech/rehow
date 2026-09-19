# Phase 5C manual Git commit decision evidence

Date: 2026-09-19

ADR 0024 and the Phase 5C1 packet define a managed-content-only, unsigned manual commit pipeline split into three independently accepted gates. Candidate construction is read-only against the active repository, uses private config-free staging, preserves unrelated `HEAD` entries and worktree changes, and refuses ambiguous ownership, advanced indexes, hostile refs, missing objects, operation states, or exhausted bounds.

The single Security Reviewer first found four issues: missing-object acceptance, unsafe reflog ordering, incomplete branch-representation binding, and unspecified limits/index/identity rules. Corrections removed `mktree --missing`, require independent object-closure proof, omit reflog writes, bind exact loose/packed representation, and freeze exhaustive markers, limits, identity, object-format, staging, and status rules. A second pass found file/directory ownership collisions and indirect/duplicate branch targets; both are now explicit refusals with adversarial tests.

Final Security Reviewer verdict: **PASS**.

Verified checks:

- `python3 scripts/check_bootstrap.py`
- `git diff --check`

No repository, Workspace, network, credential, renderer, native-command, or MCP mutation authority is added by this decision checkpoint.

## Phase 5C1 implementation evidence

The core now constructs an opaque manual-commit plan entirely in an owner-private XDG staging directory. It revalidates the retained Workspace and direct repository state, copies and proves the active index without writing it, reconstructs and validates the prior managed Workspace snapshot, writes the current managed candidate mirror, preserves unrelated `HEAD` entries byte-for-byte, and creates candidate blob/tree/unsigned-commit objects through config-free Git plumbing. Candidate tree and commit object IDs are independently recomputed for SHA-1 and SHA-256 repositories.

The returned plan exposes only a random token, branch name, managed create/update/delete counts, and an informational unrelated-change count. Its private state binds the exact repository preflight, message, candidate object IDs, and a deterministic staging snapshot. Explicit cancellation moves the held staging directory to inert quarantine with no deletion. The active Workspace, object store, index, refs, reflogs, and configuration remain unchanged; publication is deferred to Phase 5C2.

Hostile local hooks, filters, attributes, signing configuration, replacement refs, advanced or dirty indexes, ambiguous direct refs, malformed prior content, unsafe managed paths, missing/type-mismatched objects, no-op commits, staging drift, and the shared overall deadline fail closed or are proven inert by focused tests.

The final implementation review found two high issues before acceptance. A forged cache-tree could have hidden dirty index entries from Git's `write-tree`, so the private copied index now removes every accepted cache-tree extension and recomputes its checksum before semantic comparison. Preserved objects had been checked only by advertised type and size, so parent, tree, blob, and gitlink objects are now streamed under the aggregate/time limits and independently rehashed from their exact type, size, and content. A forged cache-tree over a dirty index and same-size loose-object substitution are covered by adversarial tests.

Verified implementation checks:

- `cargo fmt --all --check`
- `cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings`
- `cargo test -p rehow-core --all-features --locked` — 381 unit tests passed, 2 intentional tests ignored; all 43 error-contract, protocol, public-API/trybuild, and unknown-YAML tests passed.
- `git diff --check`

The same Security Reviewer rechecked only the two corrections and returned **PASS**.

## Phase 5C2A append-only object publication evidence

The core now enumerates the exact private loose candidate objects, creates one deterministic pack with reverse-index and delta reuse disabled, and independently verifies the v2 index, fanout, object set, offsets, pack checksum, index checksum, pack body, object types, sizes, and recomputed SHA-1/SHA-256 object IDs through a pack-only sandbox.

Under the existing cooperating Workspace writer lock, a fresh Phase 5C1 preflight and the exact staging snapshot must still match before mutation. The retained active object-store capability installs only the verified `.pack` then `.idx` pair through owner-only unnamed inodes, durable sync, and no-replace links. Existing targets are accepted only when their exact size and BLAKE3 content match. Collisions, symlinks, hardlinks, changed sources, unsafe modes, stale state, and unsupported durability fail closed. Installed objects are immutable append-only residue: this checkpoint never changes the index, branch, packed refs, reflogs, configuration, or Workspace bytes and never deletes active objects.

Verified checks:

- `cargo fmt --all --check`
- `cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings`
- `cargo test -p rehow-core --all-features --locked` — 385 unit tests passed, 2 intentional tests ignored; all 43 error-contract, protocol, public-API/trybuild, and unknown-YAML tests passed.
- `git diff --check`

Focused coverage proves exact candidate closure, append-only installation, idempotent identical installation, unchanged branch/index, candidate readability without publication, stale-plan refusal before active mutation, and preservation of a mismatched active pack collision. Phase 5C2B remains responsible for the recoverable index/direct-ref-last transaction and final confirmation surface.

## Phase 5C2B recoverable metadata publication

The private staging repository now generates the candidate index with fixed config-free `read-tree`, requires `write-tree` to reproduce the already verified candidate tree, parses the resulting bounded stage-zero v2/v3 index, removes any cache-tree extension, recomputes the object-format checksum, atomically replaces only the private staging index, and proves the stripped index still writes the candidate tree. The source is descriptor-opened and content-hashed for the later recovery transaction; the active index remains byte-identical.

Publication now installs the already verified candidate pack append-only, then records an operation-bound canonical recovery intent before changing active metadata. The intent binds the Workspace/root and Git identities, exact branch representation, before/candidate `HEAD`, config, packed-ref, index, and ref objects, candidate commit, plan hash, staging identity, monotonic generation, and recovery stage. A pending non-complete intent blocks new commit planning.

The active transaction publishes the exact index first and the direct loose branch ref last, without reflogs. Same-user races are detected through exact snapshots: before ref publication they restore the exact prior index and preserve the foreign ref; after candidate ref publication recovery only moves forward. Unborn, loose, packed-parent, SHA-1, and SHA-256 repositories are covered. Final verification rehashes the commit, validates its tree/object closure, proves candidate-index leaves equal tree leaves, and freshly reinspects current managed Workspace effects so a valid post-publication user edit remains untouched and visible as uncommitted work.

Every durable boundary is replayable. Exact staging is identity-bound and moved to inert quarantine before the intent becomes complete. Quarantine replay repeats both parent-directory syncs and the state-root sync even when the rename already happened; injected interruption tests cover the rename and every following sync boundary. Foreign or ambiguous metadata is preserved and reported as conflict rather than guessed, deleted, or overwritten.

Verified checks:

- `cargo fmt --all --check`
- `cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings`
- `cargo test -p rehow-core --all-features --locked`
- `python3 scripts/check_bootstrap.py`
- `git diff --check`

Focused coverage includes all six index/ref interruption boundaries, every quarantine durability boundary, exact intent codec/transition validation, plan refusal during recovery, valid post-ref Workspace edits, before/after-publication ref races, foreign index/ref preservation, unborn and packed branches, and SHA-256 repositories. The reused Security Reviewer found two initial gaps—successful staging retention and missing fresh Workspace-effect verification—and one retry-sync durability gap. All three were corrected; final verdict: **PASS**.
