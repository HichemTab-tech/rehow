# Threat Model

Status: bootstrap baseline; security review required at named gates
Last updated: 2026-09-19

## Scope and assets

Protected assets:

- Workspace integrity, availability, and confidentiality.
- Files outside the selected Workspace.
- User credentials, SSH agent, Git credential helpers, tokens, keys, environment, and private remote URLs.
- Host process execution and desktop session safety.
- Guide trust status (`tested`) and audit meaning.
- Correctness of revisions, backups, recovery, search results, and MCP change reports.

Components in scope: Tauri webview/IPC adapter, Rust application/core/storage/search/Git code, Markdown renderer/Attachment protocol, Workspace files, watchers, local preferences/cache/state, standalone MCP server, Git child processes, installer/package, and external opener.

Out of scope as a hard boundary: protection against a fully compromised same-user account or kernel. Rehow still minimizes accidental damage and malicious-content influence under that account.

## Trust boundaries

1. **Webview → native IPC:** renderer input is untrusted even with CSP.
2. **Native selection → Workspace capability:** native code retains the descriptor; an opaque token grants only the classified root and explicit actions. Renderer paths grant nothing.
3. **Workspace files → parser/index/renderer:** repositories and imported Markdown may be hostile.
4. **Core → operating system:** filesystem races, special files, and partial durability matter.
5. **Core → Git process:** executable/config/repository/output/network/credential helpers can execute or leak.
6. **MCP client → server:** caller may be buggy, prompt-injected, or hostile.
7. **Rehow → external opener:** URLs/files cross into other applications.
8. **Concurrent writer → revisioned mutation:** external editors, Git, desktop, and MCP do not honor one another's in-process state.

## Security invariants

- Only native-owned folder/file selection or MCP process startup configuration accepts ambient absolute paths; renderer/tool calls cannot provide them.
- Managed reads/writes use retained root capability plus validated identifiers/relative paths and revalidate at each operation.
- Managed symlinks, multiply linked files, and non-regular files are rejected; no path traversal.
- No generic shell/process/filesystem/URL IPC or MCP tool exists.
- Markdown cannot execute; raw HTML/remote media are disabled; external URLs are scheme checked and explicit.
- Mutations are revision-checked and recoverable; broad/destructive mutations require plan/effect checks, startup capability, and native out-of-band approval.
- `tested` is a native user trust assertion for an exact verification-content hash, never an MCP/client boolean or inferred validation.
- Before Phase 3 hash verification, a stored `tested` token is only an untrusted declaration. Phase 1D exposes no effective-tested accessor and no mutation/emitter route to create tested trust.
- Planned Phase 1D-C mutation capability remains private, tied to one owned reconciled artifact, accepts only closed typed intents, and returns a fully reparsed/rebound candidate rather than reusable offsets or edits. Phase 1E will reread and replan against the current revision before applying anything.
- Git actions use an audited primitive, never user/Guide-generated arguments or a shell.
- Secrets/content are excluded from logs and errors by default.

## Threats and controls

| Threat | Example | Primary controls | Required evidence |
| --- | --- | --- | --- |
| Malicious Markdown / XSS | `<script>`, event handlers, crafted URLs | Parse CommonMark/GFM in Rust; typed safe AST; raw HTML literal/disabled; React escaping; strict CSP | Malicious corpus, CSP production test, browser security review |
| Embedded/remote content | tracking image, iframe, SVG script | Remote images/embeds disabled; managed Attachment IDs; bounded raster types; SVG deferred | Network-observation test, image decode limits |
| Unsafe link | `javascript:`, encoded scheme, `file:` disclosure | Parse/normalize URL; allow `http/https/mailto`; guarded click/open; display destination | Unicode/control/encoding/link corpus |
| Path traversal | `../../.ssh/id_ed25519`, absolute path | ID-based API; normalized relative paths; dirfd capability; reject absolute/parent/NUL | Unit/property/integration tests |
| Symlink escape / TOCTOU | replace directory after check | Linux `openat2` beneath/no-magic/no-follow or component `openat` fallback; reject symlinks; final revalidation | Concurrent component/final swap adversarial tests |
| Arbitrary filesystem access | compromised webview forges `/home/user` | Native-owned dialog; descriptor-bound selection token; no path-bearing IPC/generic commands; exact Tauri capability allowlist | Forged-path IPC negatives and capability snapshot |
| Watch spoof/loss/escape | forged event path, symlinked subtree, overflow | Non-recursive descriptor-backed managed-directory watches; event contains only Workspace ID/session/kind; receive-only event capability; full canonical refresh; dirty deferral; degraded/refocus/manual fallback | Burst/new-directory/Git-ignore/degraded/stale-session tests and capability snapshot |
| Arbitrary shell execution | command block or MCP argument becomes command | No Run feature; no shell plugin; fixed argv/process code only | Search source for process sites; injection fixtures |
| Malicious Git repo | hooks, filters, fsmonitor, textconv, pager, gitfile, alternates | Hooks/pager/editor/external diff/filter avoidance; sanitized env/config; repo-root equality; bounded subprocess; staged security ADR for mutations | Malicious repo suite; Security Reviewer approval |
| Git destructive action | reset/clean/force/rebase ambiguity | No such API/args; plan-only safe action allowlist; conflict stops | Command allowlist tests and review |
| Hostile filename | control chars, invalid UTF-8, option-like names | Managed UTF-8 policy; filename normalization; IDs; pathspec `--`; escaped display | Filename corpus on temp FS |
| Credential leakage | remote URL token appears in UI/log | Redact userinfo/query; no env dump/raw stderr; protocol error mapping; output caps | Secret-canary tests across logs/results |
| Credential capture | app asks for token/key | SSH uses only the launch-held agent socket; HTTPS uses approved helper/proxy inodes plus an exact private `gh` backing descriptor when required, and validates bounded host-bound records in a Workspace-free sandbox; Rehow never parses credential bytes; prompting is disabled and failures are silent/redacted | Loopback ephemeral-agent fixture, helper/backing provenance/race/protocol corpus, mount/env tests, packaged credential canary, UI/MCP copy review |
| Remote-config execution or secret injection | URL embeds token; repository selects helper/proxy/SSH command | Phase 5B2 accepts only canonical secret-free HTTPS/SSH forms. Network commands pass the canonical URL/ref explicitly into config-free children. SSH uses only the fixed descriptor-mounted proxy/client and launch-captured agent/host keys. HTTPS provenance reads only stable global snapshots with includes disabled, rejects scoped/free-form helpers, and invokes nothing; anonymous HTTPS keeps helpers disabled | URL/name/upstream corpus, exact command/env/refspec tests, provenance/race tests, loopback SSH fixture, redacted response/error/Debug tests |
| SSH proxy argv injection | Git or a hostile caller supplies SSH flags, another remote command, or shell metacharacters | Dedicated proxy accepts only observed upload-pack or receive-pack grammar, reconstructs rather than forwards argv, invokes only the descriptor-mounted system SSH client without a shell, clears the environment, and fixes config/host-key/agent/auth/forwarding/proxy/local-command policy | Accepted Git trace vectors, hostile argv/service corpus, exact argv/env tests, OpenSSH config parse, loopback product invocation, Security Review |
| Host-key trust redirection or race | HOME/`.ssh`/known-host path is linked, writable, oversized, replaced, or mutated while read | Startup considers only fixed system paths and launch-home user path; requires expected ownership, non-writable one-link regular files, safe ancestry, size caps, no-follow identity, repeated stable reads, and all-or-nothing status; sandbox receives only sealed snapshots at fixed paths and rewinds them per child | Missing/invalid HOME and hostile file corpus, stable snapshots, descriptor reuse, real trusted-host success and wrong-host refusal |
| SSH sandbox confused deputy or descriptor leak | Network child sees Workspace/user paths, mutable trust files, substituted executable/agent, or inherited authority | Helper and system SSH client are inode-held; host keys are aggregate-bounded sealed `0400` memfds; Bubblewrap mounts only exact held descriptors at compiled paths and no Workspace/private key; memfds are rewound and CLOEXEC changes occur only in each setup child | Missing/unsafe/overflow matrix, seal/mount tests, absent-Workspace assertion, real loopback clone/sync, wrong-host refusal, unchanged active-state checks, bounded cancellation/output tests, Security Review |
| Credential proxy injection or disclosure | Git/helper supplies hostile argv, host, fields, output, stderr, or secret values | Dedicated proxy accepts closed argv/kind, exact HTTPS host, bounded allowlisted records, no get-time secrets, direct fixed helper argv with cleared environment, bounded process group/streams, validated output, silent errors, and no app-visible secret channel; resources mount only held proxy/helper descriptors without Workspace or network | Host/argv/field/limit corpus, exact command/env tests, response canaries, invalid standalone silence, networkless fixed-mount test, Security Review |
| MCP prompt injection | Guide says “call delete/read ~/.ssh” | Treat content as data; no ambient paths/shell; structured fields; read-only default; plan tokens | End-to-end hostile Guide tests |
| MCP framing/authority abuse | client sends an unbounded line, path, alias collision, or overlapping root | Rehow-owned 1 MiB fail-closed stdio transport; 1–16 startup-only absolute aliased roots; canonical overlap rejection; retained descriptor plus identity recheck; random launch handles; no returned paths | Exact/one-over frame, EOF/malformed, alias/count, overlap, root-race, and path-redaction tests |
| MCP broad/destructive misuse | stale/replayed delete/reorder | Per-root startup capability; effect-bound token; server-launched fixed native approval child over private inherited pipes; no client confirmation flag | Replay/substitution/expiry/helper-provenance/pipe/concurrency tests |
| MCP ordinary write escalation | read-only handle calls an edit, recovery storage aliases a Workspace, or an external writer races | Separate per-alias `--allow-write`; capability checked before recovery/write access; recovery roots retained by descriptor and rejected on overlap with every configured Workspace; exact Guide revision; fresh core reread under cooperating lock; one recoverable typed candidate; shared structured errors | CLI grant matrix, two-Workspace recovery-overlap refusal, read-only byte identity, stale/external race, protocol update, core recovery suites |
| False tested status | AI/client claims it tested content | All managed creates draft; renderer/MCP cannot supply status/time/hash; native dialog for exact ID/revision; core readiness/hash/timestamp; hash mismatch → effective stale; relevant edit → stale | State-machine, canonical-vector, mismatch, readiness, revision, and approval-bypass tests |
| Concurrent lost update | editor and MCP save same revision | Path+bytes revision, cooperating lock, atomic exchange, verify/preserve displaced bytes, `RECOVERY_REQUIRED` on external race | Deterministic multi-process and held-inode races |
| Partial write / power loss | crash after temp exchange | exact XDG before/new objects, durable ADR 0009 journal, fsync ordering, old/new/neither recovery | Every durability-stage fault matrix |
| Schema corruption / resource exhaustion | aliases, duplicate keys, deep YAML, huge Markdown | Restricted YAML subset, duplicate rejection, depth/byte/count limits, isolation diagnostics | Fuzz/property/limit fixtures |
| Untrusted import | source links/metadata/symlinks | read-only bounded scan, no symlink follow, preview exclusions, copy-only required flow, draft status | Import fixture matrix |
| Index poisoning/corruption | malformed doc crashes all search | per-document isolation, plain-text fields, query limits, disposable generation swap | malformed/corrupt/rebuild tests |
| Attachment bomb/special file | huge image, FIFO, device, hardlink | fresh `0600` copy, nlink=1, byte/pixel/sniff limits, inert text/raster only, never external-open file | Attachment adversarial tests |
| Backup/recovery loss | cleanup removes only good copy | validated manifest/hash, local permission, visible retention, no silent pruning, restore to new dir default | restore drills and crash tests |

## Git risk analysis

Disabling `core.hooksPath` is necessary but not sufficient. Reads/diffs and writes may consult repository/global config, external diff/textconv, fsmonitor, paging, askpass, SSH helpers, alternates, gitfiles, or clean filters. Each implemented command needs a threat inventory and exact argument/environment test.

Phasing:

1. Detect executable and repository facts with noninteractive, bounded, machine-readable read operations.
2. Add safe status/remotes/divergence/conflict reporting.
3. Configure a remote without network access by losslessly appending one validated section under an exact before-image CAS, conventional `config.lock`, atomic exchange, and same-request/native-confirmed recovery. Repository config is data only and cannot choose execution.
4. ADR 0024 accepts commit creation through private config-free `hash-object --no-filters`/`mktree`/unsigned `commit-tree`, independently verified object IDs, append-only pack installation, and a Git-specific index/ref-last recovery intent that does not write reflogs. It never invokes porcelain add/commit, configured signing, or repository hooks/filters.
5. Add isolated fetch/validated materialization/push only with native approval, protocol allowlist/state plan, credentials from explicitly trusted system/user provenance outside repository config, and no arbitrary refspec. ADR 0023 and ADR 0025 implement anonymous/authenticated HTTPS and authenticated-SSH clone, manual sync, and exact normal fast-forward push.

If a required Git mutation cannot be made without repository-controlled code execution or ambiguous history effects, that feature remains blocked and the user is directed to normal Git tooling. Product requirements do not justify weakening the host boundary.

## Markdown and Attachment policy

- Maximum limits are enforced before expensive parse/decode and documented in actionable diagnostics.
- Parser returns a typed block/inline AST; code text is plain content.
- External links never auto-open. Remote images do not load.
- Local Attachment resolution uses opaque `(workspace_handle, attachment_id)` and validates Guide ownership/hash/path.
- SVG, HTML, PDF active scripting, video autoplay, and arbitrary file preview are outside v1 inline rendering. Attachments are never externally opened; executable/unknown types are inert text/reveal/export only.

## Concurrency and recovery limitations

An uncooperative same-user process can race the last check. Directory descriptors prevent escape; on supported Linux filesystems atomic exchange retains displaced bytes and verifies them after swap. A mismatch returns recovery/conflict and preserves both versions; an editor holding an old inode can still write it, so universal CAS is not claimed. Advisory locks coordinate Rehow desktop/MCP writers only. ADR 0009's fault/race/before-image tests are mandatory.

A hostile MCP client with an explicitly granted write capability can request allowed domain edits inside that Workspace; confinement does not claim to prevent all authorized edits. Broad/destructive/Git and `tested` transitions require a native dialog in a fixed server-launched sibling process connected only by inherited anonymous pipes; the client cannot provide the approval result. A compromised same-user desktop session, process tracing/injection, or GUI automation is outside the hard boundary.

The official Rust MCP SDK owns protocol dispatch, but its convenience async stdio reader is not used because it has no finite line bound. Rehow's adapter accumulates at most 1 MiB plus the delimiter check, rejects malformed or unterminated frames, closes without echoing input, and reserves stdout exclusively for JSON-RPC. Startup refuses unless every configured root is a compatible Workspace; no runtime root addition or ambient discovery exists. Phase 6B content reads remain descriptor-confined and finite: pages cap at 100, Guide body/code payloads have independent caps, and cursors bind to the launch handle, filter, tool, and current retained view.

Phase 6E1B2 reads only a no-follow, safe-metadata sibling that matches the exact build-time BLAKE3 package digest, then retains a private non-writable sealed executable snapshot so pathname replacement and same-inode overwrite cannot change the launched bytes. The fork child sets parent-death and no-new-privileges controls, creates its own session, maps only the fixed approval pipes, binds standard streams to `/dev/null`, closes every other descriptor except the held snapshot, and uses `execveat(AT_EMPTY_PATH)` with no shell, `PATH`, loader/module injection variables, credentials, or effect data in argv/environment. The parent accepts only exact byte `1` plus EOF and zero exit before the monotonic deadline; every other result denies and the process group is killed/reaped. Plans are memory-only, random, capped, keyed to exact Workspace/operation/effect state, single-use, expiring, and cancellable. Cancellation and approval finalization linearize under the same plan lock. No MCP call can create or execute one in this phase.

Phase 6E2A gives the client only preview inputs and handle/token execution. The server binds the complete Category ID/revision order and snapshot, obtains consent out of band, refreshes and rechecks every effect after consent, and refuses replay, substitution, expiry, cancellation, or drift before mutation. It extracts the SDK request token and races cancellation against the native helper; cancellation revokes the active plan and kills/reaps the helper. The core reopens and validates the complete Workspace snapshot while holding the recovery writer lock, prepares candidates/objects, then performs the same issue-free snapshot scan again at the journal boundary with cancellation checks immediately before and after it. This closes the approval-to-mutation manifest race even against external writers that do not honor Rehow's lock. The execution request contains no effect text, path, revision, numeric order, or approval assertion.

Phase 6E2B treats Category removal as destructive even though its descriptor is recoverable. Planning proves the target is empty and not default; the private binding includes its exact snapshot, revision, ID, and resolved directory. Execution accepts only handle+token, obtains native destructive approval, then refreshes and repeats those checks. The core rechecks the issue-free snapshot twice under the writer lock, refuses unknown directory entries, moves only the descriptor to exact trash, and removes the parent nonrecursively. If an uncooperative writer adds content after the final scan, directory quarantine refuses to cascade; the journal preserves a conflict and MCP returns `RECOVERY_REQUIRED` with the operation and affected path while both foreign content and trashed descriptor remain intact. No cascade, replacement-default shortcut, path input, or client consent assertion exists.

Phase 6E2C treats Tested as a user trust assertion rather than an ordinary status edit. The private token binds the exact Workspace, Guide ID/revision, Category, managed path, operation, and effect summary. Execution accepts no Guide/status/hash/time/path/effect/consent fields, uses the sealed helper's Tested prompt, refreshes after consent, and rejects drift. Post-approval filesystem work runs off the single-thread protocol runtime so a cancellation notification can still revoke the request before the journal boundary. The core reparses the exact current bytes under the writer lock, validates Steps readiness, calculates `verify:v1`, owns trusted timestamps, and checks cancellation immediately before journal execution. AI or protocol text cannot manufacture approval.

Phase 6E2D1 accepts only an existing Attachment ID and bounded title at an exact Guide revision. The core changes one proven nested scalar and verifies every file-binding field and unknown entry remains unchanged. No file bytes, paths, hash, size, media type, new ID, broad approval, shell, Git, or network authority is added.

Phase 6E2D2 accepts only stable IDs/revision during planning and only handle+token during execution. The opaque plan binds the core-streamed device/inode, file identity/hash/size/mode/path revision, Guide candidate, and Workspace capability. Planning and native post-consent revalidation run in blocking work so current-thread protocol cancellation remains dispatchable. The recovery journal retains the approved source identity; atomic displacement is followed by held-inode/content verification. A substitution preserves both files and a persisted post-Guide conflict returns `RECOVERY_REQUIRED` with the operation and both bounded affected paths. Final-boundary cancellation prevents journal creation. Removal is recoverable trash, never permanent/recursive deletion.

Phase 6E2D3 never accepts a host or relative path. Core discovery enters only the exact descriptor-confined `attachments/<GuideId>/` directory, filters to canonical stable single-link regular orphan files, and returns an untrusted filename/size plus a random process-local token while retaining path/identity/hash authority privately. The bounded expiring token is scoped to one launch handle, Guide, and revision and is consumed once. Add requires startup write, refreshes and revalidates the exact file under the writer lock, derives all metadata in core, and only journal-updates the Guide. It never moves, replaces, deletes, opens, renders, or executes the file; drift and token substitution fail closed.

Phase 6E3A exposes no generic Git interface. `git_status` adds only a keyed process-local fingerprint over its already bounded pathless result. Commit planning requires startup write+Git-write authority because it creates owner-private candidate objects, and it reuses the Phase 5C descriptor-confined, fixed-argv, cleared-environment, hook/filter/helper/signing/network-free core. One atomic per-Workspace reservation excludes overlapping normal/recovery work before candidate construction and remains owned through native approval and apply/recovery termination. The private approval binding includes the opaque core plan token, portable Workspace ID, exact normalized message, branch, and managed/unrelated counts; the core plan remains authoritative for repository/index/ref/content identity and CAS. Apply accepts only handle+token and requires sealed native Git-write approval. Denial, expiry, replay, scope mismatch, and protocol cancellation at the final pre-intent core boundary quarantine only exact held staging; harmless append-only object residue may remain, but no index/ref/intent is published. Once the durable intent begins, core recovery owns the outcome. Recovery is separately planned and approved, binds an opaque core selector for the exact pending operation, refuses replacement by another operation, exposes no operation ID, and cannot reset, discard, rewrite, push, or select arbitrary Git state.

Phase 6E3B extends that same closed boundary to the configured upstream only. Launch-time credential/SSH provenance is captured only when Git-write authority exists. A first native approval precedes credential preparation and any network access; denial is networkless. The accepted core performs isolated fetch, complete Workspace validation, bounded ancestry classification, and cancellation before returning a retained opaque classification. Only clean remote-fast-forward pull or local-ahead push can receive a second, separately approved mutation token. Pull invokes only recoverable atomic fast-forward publication; push invokes only the exact non-force preflight/publication/postflight path and treats uncertain remote state as conflict. No URL, ref, OID, path, credential, identity, raw Git output, or Git argv crosses MCP. The shared per-Workspace reservation excludes commit, sync, and both recovery lifecycles through terminal execution. Pull recovery is pathless, networkless, and bound to the exact pending operation so a later recovery record cannot replace it.

Phase 6C1 write authority is absent unless the launcher separately names a configured alias. The sole mutation accepts no path/status/time/ID-generation field, caps its body below the transport frame, requires the exact current revision, and commits through the accepted recovery journal. A committed write whose view refresh fails is reported with its committed revision and `refreshRequired`, never described as rolled back.

Phase 6C2 adds only canonical draft Guide creation. The caller supplies no Guide ID/path/time/status/body/YAML; the core rechecks the exact Category descriptor revision and descriptor-confined global Guide-candidate count under the cooperating writer lock, generates every authoritative field, and performs one absent-only recovery create. A stale or externally changed Category or reached hard cap writes no Guide; a committed create with failed refresh returns the created ID/revision and `refreshRequired`.

Phase 6C3 adds only canonical active Category creation. The caller supplies no Category ID/path/time/order/status/YAML. Under the cooperating writer lock, the core enforces the raw 1,000-entry cap, freshly inspects the compatible Workspace, and compares an opaque snapshot covering the manifest and every valid Category path/ID/revision before an absent-only recovery create. A stale inventory, malformed/residual entry cap, or incompatible current Workspace writes no Category; a committed create with failed refresh remains explicitly committed.

Phase 6C4 adds only exact-revision Category name/optional-summary replacement. The existing typed core planner re-reads and rebinds the descriptor under the recovery writer lock, preserves path/order/status/Guides/unknown metadata, and refuses unsafe presentation or stale bytes. No generic patch, YAML, path, lifecycle, broad, destructive, shell, network, Attachment, or Git authority is exposed.

Phase 6C5 adds separate Category archive and restore, never a caller-supplied generic status. Every Category descriptor update now also freshly inspects and compares the manifest/full valid inventory under the recovery writer lock; a concurrent default-Category change therefore conflicts before archive. Paths and Guides remain fixed. Reorder/remove, generic YAML, broad/destructive, shell, network, Attachment, and Git authority remain absent.

Phase 6C6 adds separate Guide archive and restore, never a caller-supplied generic status. Both reuse exact-revision typed recovery, preserve path/body/Category/Attachments/unknown metadata, and follow the closed restoration matrix so MCP cannot assert tested. Delete, generic YAML/content, broad/destructive, shell, network, Attachment, and Git authority remain absent.

Phase 6C7 adds only exact-revision Guide mark-stale. It can reduce trust but cannot select a generic status, assert tested, add a reason/unknown field, or change content/path/Category. Archived Guides retain archived presentation while their core-owned restorable status becomes stale. Existing recovery, confinement, and content-free error controls remain unchanged.

Phase 6C8 adds separate complete Tag and alias replacement at an exact Guide revision. The core alone enforces bounds, Unicode-fold uniqueness, round-trip preservation, and automation trust reduction. Neither schema accepts the other collection or any title, summary, body, path, Category, status, YAML, Attachment, Git, shell, network, broad, or destructive control.

Phase 6C9 adds pathless Guide Category change with exact Guide and destination Category revisions. The destination descriptor is rechecked under the recovery writer lock before destination absence and source publication; its active retained state is therefore bound to the exact bytes. Rust derives both managed paths and preserves the filename. No overwrite, caller path, unrelated metadata, tested, Attachment, Git, shell, network, broad, or destructive authority is added.

Phase 6D1 adds only bounded reads over the retained Workspace view. Validation omits filesystem paths; duplicate generation has a deterministic candidate ceiling; related ranking uses bounded metadata and closed reasons. No body expansion, mutation, Git, shell, network, broad, destructive, or tested authority is added.

Phase 6D2 exposes declared managed Attachment relative paths as untrusted metadata but no bytes, host paths, handles, or mutation. Git status delegates to the accepted fixed-executable, descriptor-confined, networkless Phase 5 reader and returns no URL, raw output, environment, credential, or path. No new process primitive, network, write, broad, destructive, or tested authority is added.

The portable `verify:v1` digest is intentionally unkeyed: it detects instruction-bearing content drift but is not cryptographic proof of which person or program wrote the Markdown. Rehow guarantees that its desktop renderer and MCP APIs cannot assert Tested without native confirmation. A same-user process with direct Workspace filesystem write access can reproduce or replace the declaration and digest and is outside that hard boundary; Workspace permissions, Git review, and visible diagnostics remain the mitigations. A future provenance/signature feature would require a separate portability and key-management decision.

Network/removable filesystems may not honor local atomicity/fsync/watch semantics. Detect known filesystem characteristics where practical, warn that guarantees are degraded, use polling/reconciliation, and do not claim durable atomic behavior that the mount cannot provide.

## Security review triggers

Security Reviewer (`gpt-5.6-sol`, xhigh) approval is required before merging:

- any path resolver, storage mutation, migration, recovery, or backup change;
- Tauri capability/CSP/protocol/opener change;
- Markdown/URL/Attachment parse or rendering change;
- any process/Git/network code;
- any MCP tool, write enablement, confirmation, logging, or response-content change;
- limit relaxation, dependency with native/unsafe/parser surface, or new persistent cache/state.

## Security release gates

- Phase 1: confinement, parsing, unknown metadata, atomicity, revision, crash/race suite.
- Phase 2: dialog/root authority, inspection/import/migration, Tauri capability/CSP suite.
- Phase 3: content lifecycle, tested state, Attachment, external conflict suite.
- Phase 4: index/query resource and poisoning suite.
- Phase 5: malicious Git repository/credential/destructive-state suite per operation.
- Phase 6: MCP allowlist/prompt injection/schema/plan/replay/data-exposure suite.
- Phase 7: package permissions, install/uninstall preservation, full threat-model audit.

Open security decisions are listed in `docs/product-spec.md`; unresolved high-impact risk is a human checkpoint, not an implementation detail.
