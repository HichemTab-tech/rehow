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
