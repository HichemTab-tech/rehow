# ADR 0025: Safe Git synchronization

- Status: Partially Accepted — safe fetch, ancestry, durable fast-forward publication, and native lifecycle implemented; UI and push pending
- Date: 2026-09-19

## Context

Synchronization combines untrusted network data, repository history, credentials, portable Workspace mutation, and ref publication. `git pull` writes the active repository and worktree before Rehow can validate the incoming snapshot; `git push` may publish stale or unintended history. Repository configuration may also select hooks, helpers, proxies, filters, transports, or commands.

## Decision

Synchronization is a sequence of separately accepted gates. Every gate binds the current Workspace, direct local branch, exact Git metadata/configuration, configured upstream, and prior gate output. No gate runs pull, merge, rebase, checkout, reset, clean, stash, or a shell.

### 5D1 — isolated incoming inspection

Core reads the current branch's exact `branch.<name>.remote` and `branch.<name>.merge` plus that remote's single canonical URL from the held config with includes disabled. Duplicate, missing, local-dot, non-branch, noncanonical, credential-bearing, or otherwise ambiguous configuration is refused. Planning is single-use, redacted, and bound to the current Workspace fingerprint, manifest revision, root capability, Git directory identity, repository snapshots, direct branch, object format, managed inventory, and clean/dirty status. A pending recoverable commit blocks planning.

Apply currently accepts anonymous canonical HTTPS only. It sends the canonical URL and exact upstream branch explicitly to the same config-free, credential-disabled Bubblewrap network boundary accepted for clone. Discovery accepts exactly one full-width OID/ref record. Fetch writes only a random owner-private XDG staging repository, requests exactly the advertised OID with depth one, retains packs, disables hooks/helpers/prompts/proxies/tags/submodules/maintenance, and enforces the existing time/output/file/aggregate limits.

The complete fetched commit, tree, paths, object identities, blob bytes, Workspace schemas, relationships, and Attachments pass the accepted clone validator. The incoming Workspace ID must equal the open Workspace ID. Core repeats the full local plan binding after validation. The result exposes only `upToDate` or `remoteChanged`, whether the active worktree has local changes, and whether the portable Workspace snapshots match. URL, paths, refs, OIDs, content, Git output, credentials, and staging authority remain private. This gate never changes the active Workspace, object database, index, refs, config, reflogs, or remote-tracking refs.

### 5D2A — bounded ancestry classification

Core consumes the opaque validated inspection, honors cancellation before every successful return, and revalidates the complete local plan before and after classification. Equal object IDs are `upToDate` without another network operation. Otherwise, core deepens only the exact already-advertised object in the same private staging repository to a 4,096-commit cap, rechecks that its validated tree is unchanged, and reads both local and staged histories through config-free, hook-disabled, read-only sandboxes. The deepening child receives only a conservative fraction of the verified remaining aggregate staging budget, with fixed metadata headroom; the complete staging tree is rescanned afterward.

History output is capped and accepts only full-width object IDs headed by the expected commit. Exact shallow-file content and metadata are plan-bound and checked around local reads; staged shallow state and repository metadata are checked around staged reads. Proven containment yields `remoteFastForward` or `localAhead`; non-containment yields `diverged` only when both histories are complete. A shallow boundary or cap yields `historyLimit`, never a guessed relationship. The result retains integration authority opaquely and exposes only the ancestry class and prior bounded facts. No active content, object, index, ref, config, or reflog is changed.

### 5D2B1 — internal journaled Workspace materialization

Core has an internal-only primitive that accepts a clean, proven remote fast-forward; revalidates the local plan and staged tree; and applies the exact portable snapshot through the Workspace recovery journal. It handles create, replace, and operation-bound trash effects across the Manifest, `.gitignore`, Categories, Guides, and Attachments. Managed directories are descriptor-confined, owner-checked, non-group/world-writable, privately staged with durable operation/path/inode ownership before no-replace publication, and quarantined after failure only when that exact identity is still empty. Every staged blob is re-read and its Git object ID is recomputed; a fresh Workspace scan must reproduce the validated remote tree exactly. Active Git objects, index, refs, config, reflogs, and `HEAD` are unchanged.

This primitive remains crate-private because it is not independently crash-atomic with Git metadata publication. The next gate must install a durable sync intent before invoking it and use that intent to resume the Workspace journal plus object/index/ref publication.

### 5D2B2 — durable fast-forward publication

Core now composes the accepted Workspace journal with append-only incoming object installation and index-first/direct-ref-last Git publication. It first prepares the Workspace journal and candidate index as inert recovery objects, validates the remote commit/tree/object closure in the active object database, and installs a generation-checked sync intent before activating the Workspace journal. The intent binds the Workspace/root/Git identities, exact unchanged `HEAD`/config/packed-refs/shallow bytes, exact before/candidate index and ref bytes and modes, the Workspace plan/journal, branch, and immutable plan hash.

Recovery accepts only the exact before/before, candidate-index/before-ref, or candidate/candidate states. It replays the Workspace journal before Git publication, verifies the complete Workspace against the candidate index while holding the cooperating writer lock, revalidates candidate objects, publishes the index before the direct loose branch ref, and marks completion only after another object check. Any foreign Workspace, index, ref, static metadata, identity, or intent state becomes an explicit conflict; Rehow does not roll it back or overwrite it. Append-only packs and inert recovery objects may remain after an interrupted pre-intent attempt.

Packed-only current branches are deliberately refused by this gate. Publishing a loose shadow without a separately accepted packed-ref compatibility policy would leave Rehow's strict single-representation parser unable to reopen the result. SHA-1 and SHA-256 object widths use the same validation path; the complete SHA-256 sync fixture remains a later portability case. The boundary stays crate-private until the native lifecycle and recovery presentation are specified.

### Pending gates

### 5D2C — native sync lifecycle

The native adapter owns one `active` / `classified` / `applying` lifecycle and never exposes the classification authority. Its pathless inspection command plans, fetches, validates, and classifies against the retained open Workspace. IPC returns only a single-use token, bounded remote labels, closed ancestry, local-change state, and portable-snapshot equality; refs, OIDs, paths, URLs, output, credentials, and staging authority remain native-only. Cancellation revokes both an active inspection and any successful result that races with cancellation.

Only a clean proven `remoteFastForward` can reach apply. Apply consumes the token once, requires an OS-native confirmation, invokes the durable publication state machine, replaces the retained Workspace view only after core success, and rebuilds external-change watching. Recovery is a separate pathless, natively confirmed command that first proves an incomplete sync intent exists. Workspace switching is blocked during active/applying work and otherwise revokes retained authority. Git status reports incomplete commit or sync intent uniformly as recovery-required.

This gate deliberately adds no React UI. The next UI slice must explain unsupported packed branches, conflicts, dirty state, divergence, and history limits using only the closed native result. It must not add renderer URL/ref/OID/path authority.

### 5D2D — Reference Desk manual sync

The status bar exposes manual Sync only for a writable ready repository with an upstream. A compact, keyboard-focused panel validates the exact closed native payload and explains up-to-date, local-ahead, dirty fast-forward, divergence, history-limit, and clean fast-forward outcomes. Only the last state exposes Apply; cancellation, decline, errors, and focus restoration remain explicit. Successful native apply/recovery replaces the validated Workspace payload and reconciles the current Guide and watcher state.

Recovery remains one generic user action without adding a recovery-kind disclosure. The UI calls pathless sync recovery first; only the stable `recoveryNotRequired` result falls through to commit recovery, so the correct native confirmation is shown. All other errors stop. Renderer input/output still contains no URL, ref, OID, path, Git output, credential, content, or staging authority.

5D3 may push only an exact reviewed local commit from the expected remote OID using an explicit refspec and native confirmation. Non-fast-forward, changed remote, auth, host-key, and transport failures are preserved and reported. No force option exists.

Authenticated HTTPS/SSH requires the separately audited credential/agent provenance described by ADR 0023. Scheduled or start-time sync remains opt-in and cannot precede acceptance of the same gates used by manual sync.

## Consequences

- The first accepted slice can securely answer whether the configured upstream snapshot is valid and changed, but cannot yet modify or publish anything.
- A changed remote is called remote-fast-forward, local-ahead, or diverged only when bounded history proves that result; otherwise it is history-limited.
- Private staging is retained as exact evidence/input for the next gate; identity-safe retention/pruning remains required.
- Portable fast-forward publication is interruption-recoverable and callable only through the accepted pathless native lifecycle and bounded Reference Desk presentation.
- Unsupported authentication degrades to an actionable unavailable state rather than a token prompt.
