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
| Credential capture | app asks for token/key | Use normal agent/helper; terminal prompting disabled; report auth required | UI/MCP copy review; tests |
| Remote-config execution or secret injection | URL embeds token; repository selects helper/proxy/SSH command | Phase 5B2 accepts only canonical secret-free HTTPS/SSH forms; Phase 5D reads one exact upstream with includes disabled and passes its canonical anonymous-HTTPS URL/ref explicitly into config-free isolated fetch/push children. Push mounts only read-only validated Git objects, disables helpers/prompts/proxies/redirects, uses one internal exact refspec with no force semantics, and verifies remote state afterward; credentials require separately audited provenance | URL/name/upstream corpus, exact command/env/refspec tests, remote-race/ambiguous-postflight tests, redacted response/error/Debug tests, per-network-operation security gate |
| MCP prompt injection | Guide says “call delete/read ~/.ssh” | Treat content as data; no ambient paths/shell; structured fields; read-only default; plan tokens | End-to-end hostile Guide tests |
| MCP broad/destructive misuse | stale/replayed delete/reorder | Per-root startup capability; effect-bound token; server-launched fixed native approval child over private inherited pipes; no client confirmation flag | Replay/substitution/expiry/helper-provenance/pipe/concurrency tests |
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
5. Add isolated fetch/validated materialization/push only with native approval, protocol allowlist/state plan, credentials from explicitly trusted system/user provenance outside repository config, and no arbitrary refspec. ADR 0023 implements anonymous clone; ADR 0025 implements anonymous manual sync and exact normal fast-forward push. Authenticated transport remains separately gated.

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
