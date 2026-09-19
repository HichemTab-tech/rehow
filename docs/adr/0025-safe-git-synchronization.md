# ADR 0025: Safe Git synchronization

- Status: Partially Accepted — manual anonymous-HTTPS synchronization/push and authenticated-SSH clone implemented; authenticated sync/push pending
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

### 5D2C — native sync lifecycle

The native adapter owns one `active` / `classified` / `applying` lifecycle and never exposes the classification authority. Its pathless inspection command plans, fetches, validates, and classifies against the retained open Workspace. IPC returns only a single-use token, bounded remote labels, closed ancestry, local-change state, and portable-snapshot equality; refs, OIDs, paths, URLs, output, credentials, and staging authority remain native-only. Cancellation revokes both an active inspection and any successful result that races with cancellation.

Only a clean proven `remoteFastForward` can reach apply. Apply consumes the token once, requires an OS-native confirmation, invokes the durable publication state machine, replaces the retained Workspace view only after core success, and rebuilds external-change watching. Recovery is a separate pathless, natively confirmed command that first proves an incomplete sync intent exists. Workspace switching is blocked during active/applying work and otherwise revokes retained authority. Git status reports incomplete commit or sync intent uniformly as recovery-required.

This gate deliberately adds no React UI. The next UI slice must explain unsupported packed branches, conflicts, dirty state, divergence, and history limits using only the closed native result. It must not add renderer URL/ref/OID/path authority.

### 5D2D — Reference Desk manual sync

The status bar exposes manual Sync only for a writable ready repository with an upstream. A compact, keyboard-focused panel validates the exact closed native payload and explains up-to-date, local-ahead, dirty fast-forward, divergence, history-limit, and clean fast-forward outcomes. Only the last state exposes Apply; cancellation, decline, errors, and focus restoration remain explicit. Successful native apply/recovery replaces the validated Workspace payload and reconciles the current Guide and watcher state.

Recovery remains one generic user action without adding a recovery-kind disclosure. The UI calls pathless sync recovery first; only the stable `recoveryNotRequired` result falls through to commit recovery, so the correct native confirmation is shown. All other errors stop. Renderer input/output still contains no URL, ref, OID, path, Git output, credential, content, or staging authority.

### 5D3A — exact fast-forward push

Push consumes the same opaque, validated `localAhead` classification used by manual sync. The authority binds the exact local branch commit, the exact advertised upstream commit, configured remote, branch/ref, repository snapshots, object format, managed inventory, Workspace fingerprint, and root/Git identities. The renderer supplies only the single-use plan token; it never supplies a URL, ref, object ID, path, refspec, or credential choice.

Immediately before publication, core repeats the complete local binding and performs a fresh exact-ref discovery. A changed or missing upstream is `remoteChanged`, not a best-effort push. After native confirmation names only the bounded remote, branch, transport, and host, Git receives the internally constructed `<exact-local-oid>:<exact-upstream-ref>` refspec. Rehow uses an ordinary push with no force, force-with-lease, deletion, wildcard, mirror, tag-following, upstream-setting, atomic multi-ref, or option passthrough. A race after discovery can therefore only be rejected by the server or accepted as a genuine server-verified fast-forward; it cannot discard remote history. Success is accepted only after a second exact-ref discovery returns the planned local commit.

The Git child sees a synthetic config-free repository containing only read-only validated objects/refs and an explicit canonical URL. Repository/worktree/user/system/includes configuration, hooks, aliases, filters, submodules, maintenance, proxy selection, redirects, askpass, prompts, and credential helpers are disabled. The first accepted transport is anonymous canonical HTTPS. Output is bounded and never crosses the core/native boundary. Decline consumes the authority without network publication; once confirmed publication begins it is not presented as cancellable because the remote may have accepted before local cancellation is observed.

### 5D3B — trusted authentication provenance

Authenticated HTTPS and SSH reuse one audited transport broker across clone, sync, and push. The broker implements ADR 0023's boundary rather than enabling normal Git configuration wholesale:

#### 5D3B1 — launch SSH-agent capability

The native process now reads `SSH_AUTH_SOCK` exactly once while constructing application state. Missing input is `notConfigured`; malformed or unsafe input is `unsafe`; neither blocks normal application startup. A trusted candidate must be an absolute bounded Unix socket owned by the effective user, have exactly one link and no group/world write permission, and sit beneath a complete non-symlink ancestry that is either private/current-user-owned, non-writable/root-owned, or a root-owned sticky shared directory such as `/tmp`. Rehow checks the socket and ancestry before and after an `O_PATH | O_NOFOLLOW` capture and retains only that close-on-exec inode capability. Public status and `Debug` expose no path, descriptor, inode, or environment value.

This accepted provenance primitive performs no connect, network, SSH/Git process, key read, user-config read, IPC, or filesystem mutation. Holding the capability does not yet authorize authenticated transport. The fixed SSH proxy, host-key source, descriptor handoff, credential canaries, and operation integration remain later 5D3B gates.

#### 5D3B2A — fixed Git SSH proxy

The Workspace now compiles a dedicated `rehow-git-ssh` binary, but no product network operation invokes or packages it yet. The proxy accepts only Git's observed SSH upload-pack argv: optional single `-o SendEnv=GIT_PROTOCOL` and canonical nonzero `-p` pairs, one canonical lowercase host with an optional bounded user, and exactly one single-quoted `git-upload-pack` command whose relative or absolute path uses Rehow's existing remote-path grammar. Unknown, duplicate, non-UTF-8, receive-pack, option-like, traversal, quote, whitespace, metacharacter, or trailing command input exits 255 without diagnostics.

Accepted input is reconstructed rather than forwarded. The proxy invokes only root-owned `/usr/bin/ssh` by argv after clearing the environment. It selects `/dev/null` configuration; fixed synthetic home, agent, user-known-host, and global-known-host paths; protocol-v2 `SendEnv`; strict host verification; public-key-only batch authentication; no key files, passwords, keyboard interaction, forwarding, proxy/jump/local/known-host commands, control socket, prompt, or TTY; and fixed connection/liveness bounds. It contains no shell path. OpenSSH's own config-dump parser verifies the complete fixed option set in tests without making a connection.

This binary remains inert product infrastructure until 5D3B2B supplies identity-held bounded `known_hosts` sources and a later gate mounts the already-held agent/host-key descriptors plus the trusted packaged proxy into a network sandbox. That completed invocation boundary requires the deferred Security Reviewer and credential-canary gate.

#### 5D3B2B — trusted known-host snapshots

Native startup now also inspects the optional launch-home `.ssh/known_hosts` and the fixed system `/etc/ssh/ssh_known_hosts{,2}` sources. A configured source must be an absolute bounded regular file with one link, no group/world write permission, exact effective-user or root ownership as applicable, a safe non-symlink ancestry, and at most 1 MiB. Core opens it read-only with `O_NOFOLLOW | O_CLOEXEC`, binds path/descriptor device, inode, owner, type, mode, link count, and size, reads it twice through the held descriptor, and retains only identical bounded bytes. Missing files are `notConfigured`; any unsafe existing source makes the aggregate `unsafe` and discards otherwise trusted snapshots. Timestamp equality is deliberately not an authority input because lazy-atime/writeback behavior varies by filesystem; byte equality and stable inode metadata carry the proof.

The resulting public object reveals only a closed status and source count; `Debug` omits paths, identities, and bytes. Host-key material is never sent through renderer IPC or logs. This slice performs no OpenSSH/Git process, agent connection, network access, prompt, TOFU update, or filesystem mutation. A later 5D3B gate must create sealed synthetic files from these snapshots, bind the exact retained agent inode and packaged proxy into Bubblewrap, and prove credential/output confinement before product SSH is enabled.

#### 5D3B2C — sealed SSH sandbox resources

Native startup now also looks only for an exact sibling `rehow-git-ssh`. Missing is unavailable; an existing candidate must be an absolute, one-link regular executable owned by root or the effective user, non-writable by group/other, beneath safe non-symlink ancestry, and stable across an `O_PATH | O_NOFOLLOW | O_CLOEXEC` capture. Public status and `Debug` reveal no path or identity.

One operation may prepare SSH resources only when the launch-captured agent, host-key snapshots, and proxy are all trusted. Core clones the exact held agent/proxy descriptors, combines host keys under a hard aggregate cap, copies them into `0400` memfds, and applies write/grow/shrink/further-seal seals. Bubblewrap receives only those four exact descriptors: `--ro-bind-fd` mounts the agent socket and proxy inode, while `--ro-bind-data` mounts the sealed user and empty system host-key files at the fixed paths compiled into the proxy. Close-on-exec is cleared only in the Bubblewrap child for those descriptors; no source path is reopened.

A networkless Bubblewrap test proves the fixed socket/key/proxy types are visible and `/workspace` is absent. Missing/unsafe inputs, aggregate overflow, proxy symlink/writable mode, memfd/seal failures, and empty output fail closed. These resources remain crate-private and no clone/sync/push path can construct them yet. The first product SSH operation must add a network child, process-group cancellation/output bounds, stable auth/host-key result mapping, credential canaries, packaging evidence, and the deferred Security Reviewer before authenticated transport is accepted.

#### 5D3B3 — authenticated SSH clone

Canonical SSH clone discovery and fetch now consume the launch-captured resources through the existing bounded clone pipeline. The child enables only `protocol.ssh`, selects the fixed proxy through a synthetic path, receives no Workspace or private-key file, and preserves the established cancellation, timeout, output, staging, content-validation, and untouched-destination behavior. The exact root-owned system SSH client is captured outside Bubblewrap's user namespace and descriptor-mounted beside the helper; the proxy executes only that synthetic client path.

Host-key memfds remain sealed and are rewound in each Bubblewrap setup child before `--ro-bind-data`, so discovery and fetch safely reuse the same opaque authority. Bounded stderr is never returned; it is inspected only for fixed OpenSSH authentication and host-key failure signatures and collapsed to closed errors. Missing agent, host keys, helper, or system SSH fail before network access with actionable non-sensitive results.

The standard desktop dev and optimized no-bundle build scripts produce a one-link sibling helper, satisfying launch provenance for current builds. Final installer bundle sidecar layout remains part of the later packaging phase while Tauri bundling is disabled. A disposable loopback test uses a temporary agent, client/host keys, unprivileged `sshd`, Git repository, and compatible Workspace to prove real success and wrong-host-key refusal without public network or user credentials. The deferred Security Reviewer is required before this gate is accepted.

- SSH accepts only the launch-inherited, owner-safe Unix `SSH_AUTH_SOCK`; a compiled fixed-grammar proxy invokes the exact descriptor-mounted system SSH client with a synthetic home, no user config, strict host verification, batch mode, fixed timeouts, and bounded owner-safe `known_hosts` snapshots. It never reads private-key files or accepts proxy/local commands.
- HTTPS reconstructs only an accepted helper from root-owned system or owner-safe user Git config read with includes disabled. Executables must be root-owned absolute binaries in `/usr/bin` or `/usr/lib/git-core` with no free-form arguments, except the exact root-owned `gh auth git-credential` argv. Shell, relative, owner-writable, URL-scoped executable, proxy, header, askpass, and repository overrides are refused.
- A compiled credential proxy forwards Git's credential protocol without exposing secret fields to Rehow plans, renderer IPC, errors, `Debug`, logs, evidence, or storage. Credential and SSH processes receive no Workspace or repository mount. Stable results distinguish authentication-required, host-key, unavailable, timeout, remote-changed, and rejected-non-fast-forward outcomes without returning raw Git output.

Authenticated SSH clone is accepted only after the 5D3B3 Security Review; authenticated sync/push and HTTPS helpers remain gated on equivalent credential canaries and operation-specific review. Scheduled or start-time sync remains opt-in and cannot precede acceptance of the same gates used by manual sync.

## Consequences

- The first accepted slice can securely answer whether the configured upstream snapshot is valid and changed, but cannot yet modify or publish anything.
- A changed remote is called remote-fast-forward, local-ahead, or diverged only when bounded history proves that result; otherwise it is history-limited.
- Private staging is retained as exact evidence/input for the next gate; identity-safe retention/pruning remains required.
- Portable fast-forward publication is interruption-recoverable and callable only through the accepted pathless native lifecycle and bounded Reference Desk presentation.
- Unsupported authentication degrades to an actionable unavailable state rather than a token prompt.
- Exact publication deliberately prefers a normal server-enforced fast-forward over force-with-lease; a post-preflight race may reject safely but cannot lose remote history.
