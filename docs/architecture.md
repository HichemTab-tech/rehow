# Rehow Architecture

Status: proposed baseline; ADRs govern conflicts
Last updated: 2026-09-06

## Goals and quality attributes

Rehow optimizes for content durability, local trust, retrieval speed, recoverable mutation, desktop usability, and a single behavioral contract across desktop and MCP. Convenience must not widen filesystem or command-execution authority.

Priority order:

1. No silent content loss or workspace escape.
2. Human-readable, Git-friendly source files.
3. Fast keyboard retrieval and stable behavior under malformed/external changes.
4. A small auditable privileged surface.
5. Testable adapters and replaceable derived systems.

## System context

```text
React/Vite UI ── typed Tauri IPC ─┐
                                  ├─ rehow-core ── Workspace files
Local MCP client ── stdio ─ MCP ──┤             ├─ derived search cache
                                  │             └─ fixed-argv Git process
External editor / Git ────────────┘                     │
                                           SSH agent / credential helper
```

The user chooses a Workspace root through a native-owned dialog. The adapter retains the selected parent/root descriptor and returns an opaque, unguessable inspection capability; the renderer never receives authority merely by sending a path. Opening yields an opaque session handle. MCP launches with named root capabilities and tools use launch-scoped handles, not portable Workspace IDs as authority.

## Repository and package boundaries

```text
apps/
  desktop/                 React/Vite UI
    src-tauri/             Tauri binary and thin command/event adapter
crates/
  rehow-core/              domain/protocol/markdown/storage/search/git/application modules
  rehow-mcp/               stdio MCP protocol adapter; no desktop dependency
packages/
  contracts/               generated TS IPC contracts and UI view types
docs/                      specifications, ADRs, operations, task packets
```

This is the Phase 1A topology. `apps/desktop/src-tauri` depends on `rehow-core`; there is no separate `rehow-tauri` crate. Core modules may later become crates only through an accepted ADR—Workers must not invent that split.

### React UI

Owns view state, keyboard interaction, editor buffers, accessible rendering, and presentation. It does not receive ambient filesystem or generic shell access. It treats all Workspace content as untrusted and maps the Rust safe AST to fixed React components; it never renders an HTML string.

### Tauri adapter

Owns window lifecycle, dialogs, application preferences, IPC serialization, and event forwarding. Capabilities allow only named Rehow commands for the main window. It delegates filesystem/search/Git behavior to `rehow-core`; it does not duplicate policy.

### `rehow-core`

Owns Workspace classification, compatibility, schemas, parsing/serialization, unknown metadata preservation, identifiers, constrained paths, atomic/revisioned mutation, recovery, watching, search/index lifecycle, Git discovery/status/actions, diagnostics, and audit-safe change summaries. Its parser-neutral YAML 1.2 semantic values are separate from the private presentation/span artifact and exact source bytes; semantic values alone are never a write source. It has no Tauri or MCP dependency.

### MCP adapter

Owns MCP schema/transport mapping, configured Workspace selection, confirmation semantics, response-size limits, and tool descriptions resistant to prompt-injected content. It delegates all domain work to the core and never exposes generic file or process primitives.

ADR 0026 selects exact-pinned official `rmcp` 3.4 for the current MCP 2026-07-28 discovery lifecycle and 2025-11-25 initialize compatibility. Rehow wraps stdio in its own 1 MiB newline-frame bound because the SDK convenience transport has no finite input limit. Phase 6A opens only startup-configured canonical Workspace roots and issues random per-launch handles. ADR 0027 adds nine read-only tools with authenticated pagination, core-ranked search, bounded Guide bodies/code blocks, validation, and descriptor-confined refresh. ADRs 0028–0031 add per-alias typed Guide/Category update and create tools. ADR 0032 adds Category archive/restore plus lock-scoped manifest/inventory checks. ADRs 0033–0035 add separate Guide archive/restore, mark-stale, and complete Tag/Alias replacement. ADR 0036 adds pathless Guide Category change with an exact lock-scoped destination-descriptor prerequisite. ADR 0037 adds bounded pathless Guide validation and explainable duplicate/related discovery. ADR 0038 reuses the accepted Attachment model and confined networkless Git reader for two additional MCP reads. ADR 0039 adds explicit per-alias broad and Git-write startup declarations. ADR 0040 packages the fixed-pipe native approval endpoint; ADR 0041 adds digest-bound sibling capture, held-descriptor execution, and bounded in-memory plans; ADR 0042 exposes exact complete Category reorder; ADR 0043 adds exact empty, non-default Category removal through destructive native approval and recoverable trash. None grants generic status or tested authority. All current writes reuse the core recovery journal and grant no caller path/YAML, cascading deletion, tested, Attachment-byte, or Git-write operation.

### Contracts

Rust request/response schemas are authoritative. Generate TypeScript definitions during development and fail CI if checked-in contracts are stale. Do not hand-maintain duplicate enum/string contracts.

Portable storage/domain records and JSON protocol DTOs are deliberately distinct mapped types. This prevents serializer-dependent ambiguity and keeps YAML-specific unknown values out of an accidental JSON-shaped model. Category `order`, for example, is domain `i64` and a YAML integer, while its JSON DTO is a validated canonical decimal string generated to TypeScript as `string`; JSON numbers and JavaScript `bigint` are not the wire form. `ErrorDto` uses a code-correlated discriminated Rust enum with private constructors rather than an open details map. Exact shapes and bounded lists are in `docs/error-contract.md`.

Accepted ADR 0011 owns the dual semantic/presentation YAML strategy. Phase 1C implements only its generic parser/reconciliation artifact. ADR 0014 accepts Phase 1D-A's read-only Manifest/Category binding and records the proposed 1D-B/1D-C Guide and mutation direction. Product binders own the exact artifact and partition known/unknown entries; read-only artifacts receive no edit authority.

Phase 1D has no filesystem, revision, inventory, clock, entropy, process, network, logging, Markdown-rendering, adapter, Tauri, or MCP authority. Phase 1E must reread and revision-check current confined bytes, reparse, regenerate/revalidate the candidate, and only then use the atomic recovery primitive; cached caller-visible byte-range plans are never applied.

## Runtime data flow

### Open and inspect

1. A native `workspace_choose_location` command opens the folder dialog itself and retains a descriptor-bound selection record. For a nonexistent target it retains the canonical parent descriptor, selected basename, and parent/absence state.
2. The core inspects only that retained capability using descriptor-relative metadata, ownership/writability probes, ancestor Git discovery, and a bounded scan without following managed-path symlinks.
3. It returns a classification, compatibility facts, Git relationship, warnings, and allowed next actions. Inspection never writes.
4. Opening a compatible Workspace canonicalizes the root, validates the manifest, establishes a capability, recovers/explains interrupted writes, scans canonical sources, and starts watching.

### Read and edit

1. The core returns a Guide view plus `rev:v1:<hex>`, where the digest is BLAKE3 over the domain separator, length-framed normalized relative path, and exact canonical bytes as specified in `docs/content-model.md`.
2. The UI edits a buffer. Watch events are invalidation hints and are debounced.
3. Save sends the stable Guide ID, intended patch/full content, and expected revision.
4. The core re-reads, checks revision and schema, preserves unknown metadata, writes atomically, updates/rebuilds the index entry, and emits a change event.
5. A mismatch returns the exact `REVISION_CONFLICT` identifiers/revisions in `docs/error-contract.md`; it never silently overwrites or embeds content/comparison material in the error. The UI may explicitly re-read the current Guide through the normal read use case before offering a comparison view. The Linux exchange-based preservation algorithm and residual same-user limitation are specified in ADR 0009.

### Search

1. Query parsing normalizes Unicode/case, recognizes quoted phrases/path-like/command-like/error-like tokens, and retains literal intent.
2. Changed revisions are normalized once into versioned length-framed records in private anonymous temporary files. The retained view holds metadata plus immutable record offsets, never all Guide bodies or normalized full text in RAM; unchanged records are reused and old generations are periodically compacted.
3. Deterministic boosts rank exact normalized title, alias, command, path, and error matches deliberately; status/staleness and token coverage act as secondary signals.
4. Results include matched fields and a bounded plain normalized excerpt with Unicode-scalar highlight ranges; the renderer validates the payload and creates text/`mark` nodes, never result HTML. Malformed files remain isolated.
5. Opening or validating a Guide rereads its body through the retained root capability and exact revision, reparses it, and keeps that body only for the explicit operation. Search/index files are disposable and contain no canonical authority.

### Git

1. The core discovers Git from the GUI-safe process environment and returns `unavailable` rather than failing Workspace use.
2. Read operations run fixed `git` argument arrays with bounded output/time and parse machine formats.
3. Mutation stages are proposed, not approved by the read-only Git decision. Each requires a separate accepted ADR, an operation-specific Git state token, and malicious-repository evidence before its commands exist.
4. Authentication and conflicts return structured states; Rehow does not collect credentials or guess conflict resolution.

## Tauri command surface

Names are stable internal API candidates; schemas are generated before UI implementation.

### Workspace/session

- `workspace_choose_location(intent)` — native dialog; returns an opaque descriptor-bound `selection_handle` plus read-only inspection. The request contains no path.
- `workspace_reinspect(selection_handle)` — refreshes classification and invalidates the handle if parent/root identity changed.
- `workspace_initialize(selection_handle, request, expected_inspection)` — creates only in the still-empty/nonexistent retained target.
- `workspace_adoption_plan(selection_handle, options)` / `workspace_apply_adoption(plan_token)` — never implicit.
- `workspace_migration_plan(workspace_handle)` / `workspace_apply_migration(plan_token)` — backup required.
- `workspace_open(selection_handle, mode)` or `workspace_open_recent(recent_workspace_id, mode)` → opaque handle, epoch/generation/snapshot, diagnostics. Recent IDs resolve only in native-owned preferences.
- `workspace_close(handle)` and `workspace_refresh(handle)`.

### Category/Guide/search

- `category_list/create/update/reorder/archive/restore/remove_plan/remove`.
- `guide_list/read/create/update_title/tidy_path/change_category/status/archive/restore/delete_plan/delete/validate`.
- `import_choose_source`, `attachment_choose_source` — native dialogs returning bounded source-selection tokens.
- `attachment_add_plan/add/remove_plan/remove/view_text/view_raster/reveal_parent` — no external open/execute operation.
- `search_query`, `search_rebuild`, `related_guides`, `duplicate_candidates`.

Every mutation accepts `expected_revision`, a full inventory snapshot, or an operation-specific effect-set token. Broad/destructive calls accept a plan-bound confirmation token plus out-of-band native approval. The native desktop UI or separately installed native approval companion is the only route that can complete a pending `tested` approval.

### Git

- Accepted read surface: `git_capabilities`, `git_status`.
- Accepted onboarding, commit, sync, and push surface: pathless initialization and remote-plan/apply pairs; anonymous-HTTPS and authenticated-SSH clone; native-held anonymous-HTTPS and authenticated-SSH sync lifecycles; and `git_push_apply_open_workspace` for either accepted transport, consuming only an opaque classified sync token. Only remote/clone planning accepts a secret-free URL. Commit/sync/push responses omit paths, URLs, refs, object IDs, operation IDs, content, credentials, and Git output; every publication or replay is separately native-confirmed. A command does not exist until its operation ADR/security gate is accepted.
- ADR 0024 implements manual commit in three gates: read-only managed-snapshot/candidate construction in private XDG staging; append-only object installation plus recoverable index/ref-last CAS; then pathless native confirmation and renderer workflow. It never uses porcelain add/commit, never commits unrelated worktree changes, writes no reflog, and performs no network operation.

There is no generic `read_file`, `write_file`, `open_path`, or `run_command` command.

### Events

- `rehow://workspace-invalidated/v1` — implemented receive-only desktop event containing only Workspace ID, native watch-session generation, and `changed`/`degraded`; consumers invoke the argument-free canonical refresh.
- `rehow://workspace-health/v1` — planned richer root-unavailable/recovery health event; it does not yet exist.
- `rehow://index-progress/v1` — handle/epoch, index generation, counts/status suitable for a polite live region.
- `rehow://git-invalidated/v1` — handle/epoch and Git-state generation/token invalidation.

Events are advisory. A stale Workspace/watch-session pair is ignored; every accepted event still forces a fresh canonical snapshot, and every action verifies current state.

## Workspace storage

See `docs/content-model.md`. Canonical content is `rehow.yaml`, one-level `categories/*/category.yaml`, direct-child `categories/*/*.md`, and managed Attachments. `.rehow/trash/` and migration backups are recovery data, not active content. Runtime locks/journals/before-images and derived indexes live under XDG runtime/state/cache, outside the Workspace.

### Atomic writes

- Resolve beneath an already opened directory capability; reject `..`, absolute components, hostile encodings, and all managed-path symlinks.
- Refuse multiply linked managed targets. Store exact before-images and a prepared operation journal under mode-0700 XDG state, then `fsync` them before touching the target.
- Create a same-directory temp file with exclusive/no-follow semantics, restrictive mode, full new bytes, validation, flush, and `fsync`.
- Recheck expected identity/revision. Creation uses atomic no-replace. Linux replacement uses atomic exchange when supported so any racing external bytes are displaced into a retained file rather than discarded.
- Verify the displaced revision after exchange. If it differs, preserve both versions, attempt only the specified safe exchange-back path, return `RECOVERY_REQUIRED`, and never report silent success.
- `fsync` parent directories and advance the durable journal after every step. Multi-file operations store every before/new object and ordered step.
- On reopen, validate the outside-Workspace journal and object hashes, classify every target as old/new/neither, and idempotently finish only unambiguous accepted work; any `neither` state opens read-only recovery.

Exact format, fsync order, fallback limitations, retention, and restore behavior are normative in ADR 0009.

### Backups and migrations

Before any migration, create a timestamped snapshot under `.rehow/backups/` with a complete per-file/mode/digest inventory and completion marker, excluding `.rehow/backups` itself. Flush files/inventory/directories and re-read/verify before mutation. Never auto-migrate a newer schema or unsupported old schema. Default restore writes a newly selected empty sibling/target; in-place restore is separately planned, revision-checked, and first backs up current content.

### External changes and concurrency

Exact-pinned `notify` events are coalesced but carry no path or content across the core boundary. Core watches only retained descriptor-backed managed directories; every hint invokes the same complete descriptor-confined canonical refresh and revision-driven index reconciliation. Dirty forms defer automatic application, window refocus recovers common background event loss, and manual refresh remains available for unreliable filesystems or degraded watches. Revisions use normalized path plus exact bytes, not timestamps. Cooperating desktop/MCP writers serialize and compare revisions; uncooperative same-user writers cannot be locked universally, so the exchange/recovery protocol preserves displaced bytes and surfaces races. An open dirty editor whose revision changes enters conflict state. Comparison rereads only the selected Guide through its retained root capability and core-owned relative path, validates its stable ID, and returns a non-mutating current snapshot; Reload remains an explicit destructive choice.

Workspace diagnostics cross the desktop boundary only as a closed issue kind plus scanner-owned relative location. The payload carries at most 100 details, the complete count, and an exact truncation flag. The renderer treats locations as inert escaped text and gives kind-specific recovery guidance; it receives no absolute path or repair authority. Ambiguous interrupted-create residue is never deleted automatically.

## Search/index design

The current index keeps compact ranking descriptors in memory and puts normalized, versioned records in process-private anonymous temporary files. Guide bodies are reread only through the retained Workspace capability and exact revision. Derived records have 64 MiB per-record, 65,536 values per repeated field, and 4 GiB total live-backing limits; a one-over candidate fails before writing and cannot displace the prior index. The index contains no irreplaceable data and is dropped/rebuilt with the process or after an unavailable-record error. See ADR 0057.

Indexed fields and initial relative boosts:

| Field/match | Intent |
| --- | ---: |
| Exact title | 100 |
| Exact alias | 95 |
| Title prefix/token | 70/55 |
| Exact command token/phrase | 75 |
| Exact affected path | 75 |
| Exact error phrase | 75 |
| Tag / Category | 45 / 40 |
| Summary | 35 |
| Heading / body | 25 / 10 |

Scores are an explicit policy layered over engine relevance, not engine defaults. Tested status may break otherwise close ties but cannot bury exact stale matches. Archived Guides are excluded by default but searchable by scope. Ranking fixtures lock expected order; corpus benchmarks enforce `docs/product-spec.md` budgets.

The desktop offers Active, Tested, Draft, Stale, Archived, and All scopes. Tauri validates that closed scope set and core filters it before ranking and the result cap, so one status cannot crowd another out. Query and non-default scope are restorable route state. Core bounds each post-ranking excerpt to 220 Unicode scalars plus ellipses and 32 sorted non-overlapping Unicode-scalar ranges; Tauri exposes plain text and integers only.

## Markdown rendering

- Parse CommonMark/GFM in Rust to the versioned safe AST; React renders only fixed known nodes and never uses `dangerouslySetInnerHTML`.
- Emit embedded raw-HTML source as escaped literal/unsupported nodes. There is no HAST/HTML-string sanitization path.
- Allow only `http`, `https`, and `mailto` external links, plus validated managed Attachment references. Block `javascript`, `data`, `file`, custom command schemes, control characters, and deceptive malformed URLs.
- External links are semantic actions with destination context, never WebView navigation. A renderer request sends the open Workspace session, Guide ID, exact revision, and displayed destination; core rereads the confined Guide and proves that exact URL is retained in its safe body AST or canonical Reference metadata. It then requires OpenURI portal version 3 and `ask=true`, so every request needs an out-of-WebView application choice. The native response and errors contain no URL.
- Images load only managed Attachments by default; no remote tracking pixels.
- Copy copies literal command text only. No Run button or executable terminal integration.

## Git process policy

- Resolve a known `git` executable explicitly; GUI apps may not inherit shell startup `PATH`.
- Rehow-owned processes spawn directly with argv vectors and never construct shell text. Git's credential-helper machinery internally uses `/bin/sh` only for Rehow's compile-time absolute proxy path plus Git's closed operation word; no repository, remote, renderer, or user text contributes syntax.
- Read operations prepend `--no-pager`, `--literal-pathspecs`, and explicit disabling config for hooks/fsmonitor/external diff/textconv where applicable; they sanitize dangerous `GIT_*`, editor/pager/askpass/SSH-command variables and bound time/output.
- The compiled `rehow-git-ssh` helper is the sole SSH clone/sync/push transport. It accepts only Git's bounded upload-pack or receive-pack argv, reconstructs a fixed OpenSSH command under a cleared environment, and disables prompts, passwords, key files, forwarding, proxies, local commands, control sockets, and free-form configuration. Standard desktop build scripts place a one-link helper beside the app; final installer bundle layout remains deferred while bundles are disabled.
- The SSH resource builder accepts only launch-held trusted capabilities, snapshots host keys into sealed `0400` memfds, captures the exact system SSH client, and uses Bubblewrap descriptor mounts at fixed synthetic paths for the agent, helper, SSH client, and host keys. It never reopens a user path or mounts a Workspace. SSH clone, sync inspection, and exact push consume this opaque authority.
- HTTPS credential provenance is launch-only and native-held. It stably snapshots fixed global Git config sources, parses those bytes with includes disabled and no repository context, applies helper reset semantics, rejects scoped/multiple/free-form selections, and retains only one approved root-owned helper inode. Exact `gh` additionally requires an immutable sealed snapshot of a private standard `hosts.yml`; Rehow never parses its credential bytes.
- The compiled `rehow-git-credential` proxy is separately captured at launch. Its closed argv/environment/record grammar validates the exact HTTPS host/port, directly invokes only the descriptor-mounted retained helper with a cleared environment, bounds the process group and all streams, suppresses diagnostics, and validates output before forwarding. Clone/sync/push network children consume only those fixed mounts and never an active Workspace.
- Repository/worktree config is untrusted and cannot select executable helpers. Native startup now captures only an owner-safe, private, launch-inherited Unix `SSH_AUTH_SOCK` as a path-redacted `O_PATH` inode capability and stable bounded snapshots of owner-safe user/root `known_hosts`; it does not connect or authorize transport. Later network operations may use those retained capabilities or a helper resolved from explicitly trusted system/user configuration outside repository context, but sandbox descriptor handoff/helper execution remains gated by the operation ADR.
- Respect Git ownership and `safe.directory`; never set a wildcard exception.
- Parse porcelain v2 `-z`. Never run user aliases, hooks, submodule commands, repository-selected helpers, difftools, mergetools, or arbitrary refspecs.
- Read discovery/status, direct private initialization, metadata-only remote connection, anonymous/authenticated-HTTPS and authenticated-SSH canonical Workspace clone, managed-only commit, transport-independent manual synchronization, and exact normal fast-forward push are accepted. Packed-branch publication remains gated.
- ADR 0021 implements that baseline with fixed root-owned Git/Bubblewrap paths, a cleared environment, machine output, bounded execution, and remote names without URLs. Repository commands receive a read-only Workspace plus a synthetic config-free Git directory assembled from inherited descriptors inside an unshared mount/network namespace; object-info alternates are masked. Gitfiles and common directories are rejected.
- ADR 0022 accepts local initialization and metadata-only remote connection as separate gates. Initialization creates canonical private direct metadata without Git. Remote connection validates a secret-free HTTPS/SSH address, losslessly appends one remote section through an exact config revision and native-held plan, and uses a durable `config.lock` plus atomic exchange for same-request recovery. It performs no network or credential operation. ADR 0023 implements clone as a strict native-confirmed tree-envelope, isolated-fetch, canonical validation, journaled materialization, exact Git finalization, and renderer onboarding pipeline; ADR 0025 supplies accepted authenticated SSH and HTTPS transport.
- Accepted ADR 0024 constructs commits from an issue-free current Workspace and a valid prior managed `HEAD` snapshot. All unrelated `HEAD` entries remain exact; unrelated worktree state remains unstaged. Filter-free unsigned objects are built in private config-free staging, while a later Git-specific recovery intent installs a generated clean index and expected-before loose branch ref last. The initial commit phase does not update reflogs. Advanced/staged indexes, operation states, unsafe identity, and ambiguous managed history are refused.
- ADR 0025 push consumes only a proven `localAhead` authority, repeats local binding, checks the exact upstream immediately before publication, uses one normal explicit refspec with no force semantics, and verifies the exact upstream afterward. Its network child sees read-only Git objects plus a sealed synthetic config; ambiguous postflight state is reported rather than guessed.

## MCP architecture

The server is a separate executable using stdio by default. Stdout is protocol-only; diagnostics go to stderr with redaction. Each launch entry has a unique alias, canonical non-overlapping root, and per-Workspace read/write/destructive/Git capability. Duplicate aliases, roots, nested roots, and ambiguous portable Workspace IDs are rejected/reported. Tools use a random launch-scoped handle; `workspace_id` remains metadata, never authority.

All tools use JSON-schema-compatible request/response types and the shared `ErrorDto` in `docs/error-contract.md`. Read payloads are bounded/paginated. Mutations require expected revisions; broad/destructive operations require capability plus dry-run and a short-lived token bound to launch handle/root identity, operation, effect set, and revisions. An MCP client cannot assert `tested`; the trusted server adapter may complete the transition only after the sealed native helper records explicit user approval. See `docs/mcp-spec.md`.

Phase 6E2A is the first operation-specific approval consumer. A private complete Category-order plan binds every descriptor revision and the Category snapshot; only the sealed native helper can approve it, and the server refreshes and rechecks all effects before invoking the existing atomic recovery mutation.

Phase 6E2B applies the same sealed-plan boundary to one empty, non-default Category. The plan privately binds its resolved directory and exact snapshot/revision; execution has no caller-selected target and reuses nonrecursive recoverable trash. Default reassignment and cascading removal remain outside the operation.

Phase 6E2C binds one ready Guide's exact revision, managed path, and Category to a Tested plan. Execution accepts only handle+token, uses the dedicated native Tested language, then moves the complete refresh/recheck/core mutation into blocking work so the single-thread stdio runtime can still dispatch cancellation. The core reparses the current bytes, recalculates readiness and `verify:v1`, and journals the trusted transition. Cancellation is checked at the core journal boundary; no generic status/headless client assertion exists.

Phase 6E2D1 adds only an exact-revision typed Attachment-title mutation. The adapter accepts stable IDs and display text; the core edits the required nested title scalar while preserving every file-binding field and unknown entry. File/path/content authority remains absent.

Phase 6E2D2 reuses the accepted native Attachment-removal preview and recovery journal through an opaque core approval object. The MCP plan binds its core effect hash to stable Workspace/Guide/Attachment identities; execution accepts only handle+token and runs revalidation/streamed recovery off the protocol runtime. Cancellation linearizes immediately before journal creation.

## Preferences and local state

Recent Workspace paths, window state, theme preference, and opt-in sync schedules are application-local configuration, not portable Workspace content. Store no secrets. Losing preferences must not lose content. Validate restored window geometry against current displays and do not depend on Wayland global positioning.

The desktop stores schema-v1 preferences in an owner-only XDG configuration file using no-follow descriptor-relative access and same-directory atomic replacement. Workspace paths stay native-only and never cross renderer IPC. A returning launch re-inspects the remembered path through the normal Workspace classifier before opening it. Optional startup/scheduled Git activity invokes only the accepted isolated inspection and presents its existing review panel; mutation and push remain separately confirmed.

## Dependency and platform policy

- Pin major versions and commit lockfiles. Review generated shadcn source.
- Use the user-approved installed host toolchain—currently Node 26.2.0 and pnpm 11.3.0—without installing or switching it, with React 19, Vite 8, Vitest 5, Tailwind 4, and Tauri 2 major lines; exact dependency versions are captured by manifests and lockfiles.
- Use Rust stable pinned in `rust-toolchain.toml` once the oldest supported Tauri/plugin floor is validated.
- Linux uses WebKitGTK 4.1. Avoid essential UI that depends on bleeding-edge CSS or WebGL. Test oldest supported distro plus GNOME/KDE/wlroots Wayland and X11.
- Normal decorated/resizable windows only in v1; Wayland does not provide reliable global window coordinates or all always-on-top/effect behavior.

Primary platform references are recorded in `docs/development.md` and relevant ADRs.

## Observability and privacy

Local logs contain only fixed operation/telemetry labels, durations, request/incident IDs, and structured error codes. They never serialize `ErrorDto` text/details, entity IDs/paths, Guide bodies, command text, secrets, Attachment bytes, credential URLs, or environment values. A future user-exported diagnostic bundle requires a separate reviewed, explicit sanitization flow; nothing is uploaded automatically.

## Failure behavior

- Newer schema: read-only inspection and upgrade guidance, never a downgrade write.
- Malformed manifest: no Workspace open; diagnostic and raw-folder location remain available.
- Malformed Guide: quarantine only that entry from normal mutation/search, expose validation error and open-underlying-file action.
- Root missing/unmounted: retain session metadata, stop writes/watch, offer retry or choose another Workspace.
- Index corrupt: remove only derived index after explicit classification, rebuild, keep content available.
- Git missing/auth/conflict: content features remain fully usable.
- Partial transaction: recover idempotently or stop read-only with exact evidence; never guess after conflicting external edits.

## Architectural review triggers

Require Architect plus Security Reviewer approval for changes to Workspace paths/schema/migrations, atomicity/revisions, Markdown/link policy, Git process policy, MCP mutation/confirmation, Attachment access, logging/redaction, or Tauri capabilities. Major navigation/state-model changes also require Design Reviewer approval.
