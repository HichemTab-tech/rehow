# ADR 0024: Safe managed Git commit

- Status: Accepted for phased implementation
- Date: 2026-09-19

## Context

Rehow must create deliberate commits without running repository hooks, clean filters, signing programs, aliases, editors, pagers, credential helpers, or other repository-selected code. Ordinary `git add` and `git commit` are too broad: they can consult hostile configuration and attributes, alter an existing staged index, and include files outside Rehow's portable content.

The operation must commit only the current canonical Workspace while preserving unrelated tracked and untracked worktree changes. It must also survive crashes without silently moving a branch to a tree different from the one the user approved.

## Decision

Implement manual commit as three separately accepted gates. No network, credential, fetch, push, merge, checkout, reset, clean, stash, signing, or arbitrary ref operation is part of this ADR.

### 5C1 — plan and construct candidate objects

The eventual pathless native planning adapter accepts only a commit message for the currently open Workspace. The message is trimmed, 1–200 Unicode scalars, one line, and rejects NUL, control, bidi-control, noncharacter, and line-separator code points. Renderer responses contain only a random one-use plan token, safe branch name, managed create/update/delete counts, and an unrelated-worktree-change count—never paths, object IDs, index bytes, Git output, or content.

Planning requires:

- a writable, issue-free compatible Workspace and direct retained repository capability;
- symbolic `HEAD` naming one canonical `refs/heads/*` branch; detached or unsafe refs are refused; the target branch is either absent from both loose and packed storage for an unborn branch, one direct loose canonical OID, or one direct packed canonical OID—symbolic/malformed branch targets and duplicate loose-plus-packed or packed entries are refused;
- no merge, rebase, cherry-pick, revert, bisect, sequencer, unresolved-conflict, or Git lock state: `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`, `REBASE_HEAD`, `BISECT_LOG`, `BISECT_START`, `AUTO_MERGE`, `rebase-apply/`, `rebase-merge/`, `sequencer/`, `index.lock`, `HEAD.lock`, `packed-refs.lock`, `config.lock`, `shallow.lock`, and the target loose-ref lock are all refused by descriptor type/identity;
- an absent index only for an unborn branch, otherwise a checksum-valid index v2/v3 with at most 50,000 stage-zero entries and 64 MiB exact bytes; Rehow parses its bounds/flags/extensions, permits only the optional cache-tree extension, and uses a private copy plus config-free `write-tree` to require semantic equality with `HEAD`; staged, unmerged, intent-to-add, split/sparse index, fsmonitor, untracked-cache, EOIE/IEOT, or unknown extensions are refused;
- a valid current managed inventory and a prior `HEAD` managed set that is either entirely absent or a valid compatible snapshot; partial/malformed prior state is refused;
- bounded local-config `user.name` and `user.email`, parsed with includes disabled and no fallback to environment/global config; the name is 1–200 safe-display scalars without leading/trailing whitespace or `<`/`>`, and email is 3–254 canonical ASCII bytes with one nonempty local/domain split and no whitespace/control/`<`/`>`; missing, duplicate, unsafe, or ambiguous identity is actionable refusal.

The managed set is exactly `rehow.yaml`, Category descriptors, Guide Markdown, and declared Attachment bytes. Rehow does not take ownership of `.gitignore` or any other root file. A bounded NUL-framed `ls-tree -r -t -l --full-tree` snapshot accepts at most 50,000 entries, 64 MiB listing output, 4,096 path bytes, and the repository's exact object-ID width; names remain opaque bytes and are never logged or sent over IPC. Every preserved blob/tree/commit entry is independently batch-checked for mode-compatible type and presence through the exact synthetic active object store. Missing/promisor-only objects, type mismatch, corrupt objects, or repository alternates are refused without network access. The candidate tree preserves every `HEAD` entry outside the managed set by exact mode/name/OID, replaces only paths proven to belong to the prior valid managed snapshot, and removes only those prior managed paths no longer current. Claiming a current managed path that was not previously managed is refused when any unrelated `HEAD` entry occupies that exact path or an ancestor/descendant path, so file/directory collisions never silently transfer ownership or discard content. Unrelated tracked worktree modifications and untracked files remain untouched and uncommitted. Current or prior malformed managed state blocks the operation rather than guessing deletion ownership.

Candidate construction occurs only in an owner-private XDG staging repository created by core without Git templates. Root-owned Git and Bubblewrap run with a cleared environment, empty system/global/local config, no worktree, no network, no repository hooks/attributes, and fixed bounded argv. The exact retained repository object store is exposed read-only as an explicit synthetic alternate; repository alternates and inherited Git environment are absent. `hash-object --stdin --no-filters`, NUL-framed `mktree` without `--missing`, and unsigned `commit-tree` create exact blobs/trees/commit. Author and committer are the validated local identity; timestamp uses Rehow's current clock with UTC offset. The commit has zero parents only for an unborn branch and otherwise exactly the planned `HEAD` parent. Candidate objects and every derived OID are independently verified for the canonical repository object format, limited to SHA-1 or SHA-256; config declaration, `HEAD`/tree/index widths, staging format, and recomputation must agree. An unchanged managed tree returns `nothingToCommit`; Rehow does not create empty commits. No active repository byte changes in 5C1.

Every local subprocess has a 30-second bound and the complete plan has a 2-minute bound. Tree/status output is capped at 64 MiB; all other retained stdout/stderr is capped at 1 MiB per stream. Current managed bytes retain the existing 2 GiB/10,000-file Workspace caps; candidate staging permits at most 100,000 objects, 60,000 files, and 3 GiB. At most 8 recognized commit staging/quarantine operations and 8 GiB aggregate may exist beneath the owner-only state root; a new/replacement plan refuses with an actionable storage-limit error once either bound is reached. Cancellation/replacement moves only exact descriptor-identified staging to inert quarantine and performs no automatic deletion.

The unrelated-worktree summary comes only from bounded porcelain-v2 `-z` status in the existing config-free, hook/helper-disabled sandbox with `core.autocrlf=false`, external filters absent because repository/system/global config is unavailable, and submodules ignored. Paths remain opaque bytes and are classified against the exact current/prior managed sets only to count unrelated records. This summary is plan-bound and informational; it never selects candidate entries or authorizes a write.

The native-held plan binds repository/root identities, Workspace ID and complete managed path/revision/content effect set, exact `HEAD` bytes, symbolic ref and expected OID/unborn state, the single direct target representation or exact unborn absence, target loose-ref presence/bytes, exact `packed-refs` presence/revision and target resolution, relevant operation-marker absence, exact index bytes/tree, local-config revision plus safe identity, normalized message, prior managed snapshot, candidate tree/commit, unrelated-change summary, object format, staging identity, and launch epoch. Plan replacement/cancellation never name-deletes staging; exact recognized staging is retained or moved to owner-only inert quarantine.

### 5C2 — append objects and publish metadata

Apply first revalidates every 5C1 binding under the cooperating writer lock. It packs exactly the newly constructed objects in private staging, verifies the pack/index pair, and installs it append-only with no-replace-or-exact-verify semantics. Installed objects may become harmless unreachable residue after a later refusal; Rehow never name-deletes object-store data.

An operation-bound XDG `GitCommit` intent then coordinates exact index and loose branch-ref before/candidate bytes. It is separate from portable Workspace recovery and permits only those planned direct Git metadata paths. The generated ordinary index must write the candidate tree and contain no unmerged/intent-to-add/sparse state. This initial safe-commit phase writes neither the branch reflog nor `logs/HEAD`; configured/log-directory state is not opened or changed. Reflog support requires a later atomicity decision and cannot be silently enabled.

Publication verifies root/Git identities, symbolic `HEAD`, expected branch OID/unborn state, exact target loose-ref presence/bytes, exact `packed-refs` revision/resolution, exact index before-image, operation-marker absence, and the final managed effect set before every transition. It installs the index candidate, then moves/creates the loose branch ref last using expected-before CAS; a packed branch is shadowed only while the exact planned `packed-refs` bytes still resolve it to the planned OID, and `packed-refs` is never rewritten. Every target and parent is durably synced. Replay accepts only exact before/candidate states. If ref CAS loses while the index remains the exact candidate, replay restores the exact saved index before-image and records a conflict; if either index or ref is foreign it overwrites neither. Once the ref candidate is durable, replay only completes/validates forward and never rewinds the branch. While an incomplete `GitCommit` intent exists, Rehow Git mutations refuse and status reports recovery required.

After ref publication, Rehow verifies the branch OID, index tree, candidate tree, and current managed effect set. A same-user external edit after the final preflight is reported as a new worktree change, not rolled back. If only the approved managed changes existed, status is clean; unrelated changes retain their prior semantic status.

### 5C3 — native and renderer workflow

The pathless native apply command consumes one matching plan and presents a parented confirmation containing the exact message, branch, managed create/update/delete counts, unrelated-change count, unsigned-commit policy, and statement that no push/network occurs. Decline consumes the plan without mutation. Apply runs off the UI thread; cancellation is allowed before metadata publication begins and unavailable after the recoverable intent starts.

The Reference Desk offers a compact manual commit form only for a ready repository. It shows bounded plan facts, busy/recovery/errors through polite status, and refreshes Workspace/Git status after success. Native IPC remains path/OID/content/Git-output free. MCP commit stays absent until a later capability/approval gate reuses the same core plan.

## Consequences

- Hooks, filters, attributes, signing, helpers, and repository-selected programs cannot participate in commits.
- Users with a staged index, advanced index extensions, missing local identity, detached `HEAD`, conflicts, incomplete/promisor objects, operation state, or staging-quota exhaustion use normal Git tooling or correct the state explicitly.
- Rehow-created commits are unsigned in this phase. Signing requires its own executable/key/confirmation decision.
- Rehow-created commits do not update reflogs in this phase; the commit remains reachable from the updated branch and Rehow's durable intent records recovery facts without becoming portable history.
- Append-only orphan objects are preferable to deleting potentially shared Git data.
- Exact candidate construction and recoverable ref/index publication are more work than porcelain, but preserve unrelated changes and make the approved effect auditable.

## Rejected

- `git add`, `git commit`, shell commands, or user aliases.
- Temporarily moving/clearing the user's index and restoring it afterward.
- Committing all worktree files, trusting `.gitignore`, or auto-resolving conflicts.
- Reading global identity/config without a separately audited owner-safe capability.
- Invoking configured commit signing or hooks.
- Ref update without expected-OID/index/effect-set CAS and recoverable intent.
