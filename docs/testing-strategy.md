# Testing Strategy

Status: bootstrap baseline
Last updated: 2026-09-06

## Principles

- Test at the lowest boundary that proves the behavior, then add cross-boundary coverage for contracts and risks.
- Filesystem, Git, import, crash, and MCP tests use isolated temporary directories/repositories and deterministic fixtures. Never point tests at a real user Workspace.
- Security invariants are executable tests, not review-only claims.
- Production performance and visual/accessibility behavior need production-like builds.
- A flaky or skipped required test is a known failure and blocks its gate.

## Test layers

### Rust unit and property tests

Crate-local tests cover IDs, schemas, lifecycle transitions, error mapping, restricted YAML/frontmatter, unknown metadata, path normalization, link policy, ranking policy, plan-token binding, redaction, and Git argument construction. Use property tests/fuzz targets for path, parser, serializer, and state-machine inputs where they materially expand hostile coverage.

### Rust integration tests

Use per-test temporary roots and subprocess helpers for:

- Workspace inspection/initialization/version/migration/adoption/import;
- dirfd confinement and symlink/component races;
- atomic writes, revision conflicts, multi-process locks, fault injection, and recovery;
- watch coalescing/loss/reconciliation and index generation/corruption;
- Git repository states and malicious configuration;
- MCP application use cases independent of transport.

Fixtures must validate both observable results and exact filesystem aftermath, including no writes on failure.

### Vitest UI tests

React Testing Library + user-event (introduced by an approved packet) cover:

- first-launch and every Workspace classification;
- search focus/query/result keyboard loop and state restoration;
- Guide reader hierarchy, command copy, link/Attachment guard, validation messages;
- create/edit/status flows, unsaved navigation, stale revision, compare/recovery states;
- Category/Tag operations and destructive confirmation;
- index/Git progress/errors via polite announcements;
- pane breakpoints and accessible names/roles/focus restoration.

Mock at the generated IPC client boundary, not individual internal hooks, so tests exercise view state and error-code mapping.

### Contract tests

- Generate TypeScript from the versioned Rust protocol crate and fail on uncommitted drift.
- Snapshot Workspace/Category/Guide JSON/YAML compatibility fixtures and public error details.
- Run the same application-use-case fixtures through Tauri adapter tests and MCP handler tests.
- Verify older clients receive stable fields/codes or an explicit version incompatibility.
- Cover every `ErrorDto` code with exact keys/types, required-property rejection, optional omission, code/retryability mismatch rejection, forbidden-control/display-value validation, and adapter parity. Phase 1B does not claim raw filesystem/Git redaction tests; those are mandatory when their reviewed constructors are introduced. `BoundedList` vectors cover exact 2/2, truncated 40/32, 33/32, 32/32, 0/0, more than 32 items, and every inconsistent count/flag combination.
- Cover Category order at `i64` minimum/maximum, zero, JavaScript's safe-integer boundary and one beyond; reject JSON numbers, `+1`, `01`, `-0`, whitespace, and overflow strings while YAML remains an integer.

### YAML semantic and presentation tests

ADR 0011's parser gate includes exact-limit and one-over byte/depth/map/sequence/node/scalar inputs; invalid UTF-8 and BOM locations; LF/CRLF/bare-CR/mixed newline envelopes; missing/extra delimiters and separators; multiple documents; every anchor/alias/merge/custom-tag form; accepted explicit core tags; and semantic duplicate keys across alternate scalar spellings, nesting, flow/block collections, complex keys, and comments.

Unknown-value fixtures cover null, Boolean, string, arbitrary-size integers within the scalar cap, lossless finite floats, infinities/NaN/negative zero, ordered nested maps/sequences, and complex keys. Presentation fixtures prove no-op body-save identity, trusted deterministic emitter output, and safe/refused edits over comments, unknown descendants, key order, quoting, Unicode, BOM, newline style, and final-newline absence. Property/fuzz tests assert bounded termination/work, no panic, reparsing, exact declared semantic change, and byte identity outside authorized spans.

Phase 1D follows ADR 0014's named aggregate gate: exact/one-over scalar and collection limits; Unicode 16.0.0 trim/case-fold vectors; every status/timestamp companion matrix; exact known-key classification; unknown-root/child preservation; hostile URL and Attachment metadata grammars; read-only binding without edit authority; and production-backed replace/insert/remove aftermath fixtures. Candidate properties prove exact requested effects, unchanged identity/schema/creation time, byte identity outside authorized spans, body/unknown preservation, deterministic refusal, and bounded iterative work. Compile-fail, rustdoc, source, dependency, and generated-contract audits prove the authority and trait absences.

### MCP transport tests

Spawn the actual binary over stdio. Assert initialize/handshake, deterministic tool schemas, protocol-only stdout, stderr redaction, cancellation/shutdown, pagination/limits, read-only/write mode, errors, and end-to-end mutation files. Use a hostile MCP client harness for traversal, oversized messages, plan replay/substitution, tested-confirmation bypass, prompt-injected Guide content, and concurrent writers.

The native approval boundary additionally compiles isolated protocol fixtures in temporary directories to prove held-inode execution across sibling replacement, exact one-byte/zero-exit acceptance, trailing/nonzero rejection, expiry/cancellation process-group termination, and child reaping. Provenance tests cover missing/malformed digests, symlinks, link count, modes, ownership policy, size, and changed-during-hash identity. Helper endpoint tests separately inspect fixed descriptors and headless refusal.

Phase 6A first proves the current `2026-07-28` discovery lifecycle and legacy `2025-11-25` initialize lifecycle against the exact two-tool read-only surface. It tests the 1 MiB frame boundary at exact/one-over sizes, CRLF, malformed JSON, unterminated EOF, stdout purity, unknown handles, alias/count limits, duplicate/nested canonical roots, incompatible Workspace classifications, launch-handle/path redaction, and clean disconnect. All roots are disposable temporary Workspaces; no test may open personal content.

Phase 6B expands this to an exact nine-tool read-only surface. Focused tests cover real protocol calls for Category listing, ranked search, bounded Guide bodies/code extraction, diagnostics, and tool/filter/view-bound cursors; full core search and descriptor-refresh suites remain shared conformance evidence.

ADR 0057's search-memory gate additionally covers the versioned record codec (Unicode/NUL, exact round-trip, every truncation, invalid UTF-8, trailing/corrupt input, forged/over-limit lengths and counts), pre-allocation count validation, pre-write record/physical prior-plus-candidate aggregate one-over refusal including the compaction path, positional offset/length failure mapping, revision-based record reuse, generation compaction, descriptor-confined lazy Guide reads, and the release-only 5,000-Guide benchmark. The applicable production budgets remain 8 s cold rebuild, 50 ms warm backend p95, 300 MiB peak RSS, and 150 ms single-Guide index reconciliation on the deterministic 52.7 MiB corpus.

Phase 6C1 adds an exact ten-tool surface. Tests prove separate per-alias startup write authority, read-only byte identity, complete typed Guide updates through real protocol framing, expected-revision conflicts, shared structured domain errors, and recovery-journal reuse. Existing core recovery interruption, unsafe-presentation, unknown-metadata, resource-limit, and tested-to-stale tests remain part of the full gate.

Phase 6C2 adds an exact eleven-tool surface. Tests prove canonical draft-only creation, absence of caller ID/path/time/status/body authority, exact Category descriptor prerequisites, deterministic external Category-change refusal under the writer lock with no Guide write, stale-view refusal at the descriptor-confined 10,000-candidate cap, and accurate committed-refresh reporting. Existing core canonical emission, absent-only create, interruption, confinement, and limit suites remain part of the full gate.

Phase 6C3 adds an exact twelve-tool surface. Tests prove the list result carries a complete-inventory Category snapshot, canonical core-owned creation, absence of caller ID/path/order/status/time authority, deterministic external Category-change refusal under the writer lock with no directory, stale-view refusal at the raw 1,000-entry cap, and accurate committed-refresh reporting. Existing core Category emission, journal interruption, confinement, and limit suites remain part of the full gate.

Phase 6C4 adds an exact thirteen-tool surface. Tests prove read-only denial, exact-revision typed Category name/optional-summary replacement through real protocol framing, stable core-owned path/order/status, refreshed inventory snapshots, and deterministic external-edit refusal without overwrite. Existing core no-op, presentation, unknown-metadata, recovery, and confinement suites remain part of the full gate.

Phase 6C5 adds an exact fifteen-tool surface. Tests prove separate archive/restore schemas, read-only denial, reversible lifecycle through typed recovery, fixed directories/Guides, default refusal, a concurrent manifest/default change caught under the writer lock, and real protocol archive. Existing Category-details, lifecycle, snapshot, recovery, and confinement suites remain part of the full gate.

Phase 6C6 adds an exact seventeen-tool surface. Tests prove separate Guide archive/restore schemas, read-only denial, reversible lifecycle through typed recovery, stable Markdown path/body/Category, declared/effective status output, and real protocol archive. Existing stale-revision, tested/stale restoration, presentation, recovery, and confinement suites remain part of the full gate.

Phase 6C7 adds an exact eighteen-tool surface. Tests prove mark-stale has no generic status/reason/content authority, capability checks precede parsing, exact revisions conflict safely, the draft/tested/stale/archived matrix is closed, archived restore state is reported, path/body/Category are preserved, and real protocol execution succeeds. Existing recovery and confinement suites remain part of the full gate.

Phase 6C8 adds an exact twenty-tool surface. Tests prove separate Tag/Alias schemas, capability-before-parse denial, complete ordered replacement and clearing, Unicode-fold duplicate refusal with byte identity, no-op revision stability, stale conflicts, tested-to-stale behavior, unrelated-field/unknown-metadata preservation, and real protocol execution. Existing recovery and confinement suites remain part of the full gate.

Phase 6C9 adds an exact twenty-one-tool surface. Tests prove no caller path/metadata authority, capability-before-parse denial, exact Guide and destination Category revisions, lock-scoped concurrent destination-archive refusal, verified same-Category no-op, destination-first move, stable ID/filename/content, tested-to-stale behavior, stale conflict correlation, and real protocol execution. Existing interruption, destination-collision, no-follow, recovery, and confinement suites remain part of the full gate.

Phase 6D1 adds an exact twenty-four-tool surface. Tests prove pathless validation with warning/error grouping, conservative normalized title/alias duplicate reasons, exact candidate/comparison ceilings, explicit-over-metadata related ranking, authenticated pagination, strict read-only schemas, and real protocol routing.

Phase 6D2 adds an exact twenty-six-tool surface. Tests prove bounded Attachment metadata without bytes, isolated-repository Git branch/change/remote-name state, URL/path non-disclosure, strict input schemas, and real protocol routing through the already accepted networkless core reader.

### End-to-end desktop tests

Use the most stable Tauri/WebDriver-compatible automation available at Phase 2 and retain manual smoke coverage where Linux WebKit automation is incomplete. Tests launch the production-like native app against a temporary Workspace and exercise open/create/search/read/edit/restart/conflict. Never rely exclusively on the Vite browser view because native IPC, CSP, WebKitGTK, window behavior, and file dialogs are product behavior.

External Guide conflict coverage proves that the core can reread the selected on-disk Guide without mutating the retained view, that the native payload remains pathless and safe-Markdown-backed, and that hostile draft/disk text stays inert in the comparison. Reload requires explicit confirmation; Keep Draft leaves the editor state intact.

Guarded Guide-file opening tests prove exact revision and stable-inode retention, link refusal, pathless native results/errors, stale renderer-result invalidation, and a fixed read-only portal handoff. No test opens personal content or grants a renderer-selected path.

Release workflow tests require YAML parse success, immutable full-commit action references, accepted/rejected tag-version fixtures, exact three-file inventory, checksum grammar/hash verification, and proof that repository code runs only in the read-only build job. The first hosted run additionally proves Ubuntu runner packages and GitHub event/context behavior; local workflow inspection cannot substitute for that evidence.

## Required coverage matrix

| Area | Minimum cases |
| --- | --- |
| Detection | nonexistent, empty, current, supported old, newer, Markdown collection, unrelated, non-writable, malformed, Git, nested Git |
| Initialization | changed-after-preview, collision, partial failure, permissions, existing files, deterministic manifest |
| Import/adoption | safe Markdown, frontmatter variants, source unchanged, symlink/special file, collision, oversize, rollback |
| Compatibility/migration | each supported version, unknown fields, backup validation, interrupted/repeated migration, newer refusal |
| Markdown | known/unknown fields, comments/order strategy, Unicode/BOM/CRLF, duplicate keys, aliases/tags, deep/large input, reserved sections, malicious HTML/links |
| Guide reader context | selected-Guide-only metadata payload; warnings/prerequisites/applicability/files/relationships/References/verification; escaping, long text, missing related Guide, guarded Reference open |
| Category/Guide | every lifecycle/status/move/rename/reorder/archive/restore/delete path and invalid transition |
| Tested trust | AI/default draft, missing/false/wrong confirmation, explicit success, stale→tested, no implicit transitions |
| Search | field extraction, exact title/alias/command/path/error order, typo/prefix policy, archived scope, malformed docs, incremental/rebuild/corrupt index |
| External/concurrent | atomic-save event sequences, dirty draft, rapid rename, watch overflow/loss, path+byte revision, cooperating-writer winner, external/held-inode race with displaced bytes retained |
| Atomic/recovery | injected failure before/after object/journal/temp/exchange/directory/commit sync; all-old/all-new/mixed/neither; hostile journal |
| Desktop preferences | schema/interval rejection; non-UTF-8 remembered path round trip; owner-only modes; symlink/oversize refusal; closed pathless renderer payloads |
| Confinement | absolute/parent/NUL/Unicode tricks, symlink each component/final, race swap, hardlinks, special files, non-UTF-8 names |
| Git | unavailable, clean/dirty/untracked, branch/detached, remotes/redaction, ahead/behind/diverged, merge/rebase conflict, unsafe owner, nested repo, auth/offline, timeout/output cap, hostile hook/filter/fsmonitor/textconv/pager/env |

Phase 5A additionally proves that repository hook/fsmonitor/clean/process/include configuration cannot create a sentinel, secret-bearing remote URLs never cross the core boundary, direct gitfiles/common directories/object alternates are rejected, a post-check alternates race is masked, non-UTF-8 path bytes stay opaque, tracking divergence is explicit, and status uses retained descriptor roots with no optional lock.

Phase 5B1 proves that local initialization invokes no Git subprocess; creates only owner-private canonical direct metadata; binds a random single-use plan to the retained root, Workspace identity, and current manifest revision; rejects stale/replayed plans, symlinks, files, and unknown residue; and resumes exact marker-bound interruptions after the marker, directory, and file durability stages.

Phase 5B2 proves strict secret-free HTTPS/SSH/SCP grammar; remote-name bounds; existing-config byte preservation; Git no-include/name-only validation; same-name refusal; config revision and retained-root binding; owner/symlink/hardlink/size denial; single-use native confirmation; exact `config.lock` pre/post-exchange recovery; stale/raced-state rollback without overwrite; directory durability; and absence of network, credential, URL, path, or secret-bearing output across core Debug, IPC, errors, UI, and evidence.

Phase 5B3A proves full-width consistent SHA-1/SHA-256 identifiers, exact NUL framing and byte order, canonical Workspace-only paths, regular non-executable blob types, required manifest, exact/one-over per-file/count/managed/Attachment/listing limits, and redacted effect hashing without adding filesystem/process/network authority. Phase 5B3B/5B3C add hostile remote/ref advertisement, config/environment isolation, agent/helper provenance, credential canaries, complete streamed blob/schema/relationship validation, cancellation/time/output bounds, marker-bound interruption, journal replay, destination races, scanner equivalence, and expected-OID ref/config CAS before clone acceptance.

Phase 5D1 reuses the accepted clone network/content validator and adds exact configured-upstream parsing, single-value/branch/ref enforcement, full local plan binding and post-fetch revalidation, Workspace-ID equality, redacted inspection facts, dirty-state reporting, and proof that active Workspace bytes, objects, index, refs, config, and reflogs remain unchanged. Phase 5D2 adds capped ancestry classification, journaled Workspace materialization, durable append-only object/index/ref publication, pathless native lifecycle, and closed Reference Desk presentation. Phase 5D3A proves local-ahead-only authority, exact pre/post remote checks, one internal explicit refspec, normal server-enforced fast-forward semantics, absence of force/delete/tag/options, config/helper/prompt/proxy isolation, single-use native confirmation, remote-race refusal, ambiguous-postflight reporting, and closed UI payloads. Phase 5D3B1 proves missing/relative/oversized/non-socket/symlink/loose-mode agent refusal, safe ancestry, descriptor/path identity agreement, retained-inode behavior after path replacement, and path-redacted `Debug`. Phase 5D3B2A proves exact observed Git upload-pack argv acceptance, hostile option/destination/command/path/non-UTF-8 rejection, fixed OpenSSH argv/environment, no shell, and successful config parsing without a connection. Phase 5D3B2B proves optional source handling, invalid HOME, user/root file ownership/mode/type/link/ancestry limits, exact/one-over size, repeated stable reads, immutable snapshots, all-or-nothing unsafe aggregation, and redacted status. Phase 5D3B2C proves sibling proxy provenance, all-trusted prerequisite mapping, aggregate caps, mandatory memfd seals/modes, exact descriptor-only Bubblewrap mounts, networkless fixed-path visibility, absent Workspace, and empty output. Phase 5D3B3 adds exact system-SSH capture, one-link helper build placement, SSH-only protocol/env construction, reusable descriptor handoff, bounded closed auth/host-key errors, and a real disposable loopback agent/sshd/repository success plus wrong-host refusal with untouched destinations. The fixture selects only fixed Ubuntu `/usr/sbin/sshd` or Arch `/usr/bin/sshd`, reads no host daemon configuration, listens only on IPv4 loopback, and caps early-exit stderr at 8 KiB; both CI workflows install OpenSSH explicitly and create Ubuntu's fixed root-owned privilege-separation directory.

Phase 5D3C1 extends that real fixture through SSH sync discovery, initial fetch, bounded history deepening, and ancestry classification while byte-checking the active manifest, `HEAD`, index, and config before/after and refusing a sealed wrong host key.

Phase 5D3C2 extends the fixed proxy corpus with exact receive-pack acceptance and all other service rejection, verifies SSH-only push command policy and the existing no-force refspec, and uses the loopback bare repository to prove wrong-host refusal leaves the remote ref exact before publishing the validated local-ahead commit and verifying the exact postflight ref.

Phase 5D3D1 proves stable bounded system/user global-config snapshots; include suppression; ordered empty-helper reset; exact named, absolute, and reconstructed `gh` acceptance; scoped/multiple/shell/argument/relative/unknown rejection; config/helper ownership, mode, link, ancestry, replacement, input, and parser-output bounds; redacted status; and native-only launch retention. It invokes no helper and uses no credential or network.

Phase 5D3D2 proves exact proxy argv/helper-kind/host binding; bounded EOF/LF request and response records; duplicate, unknown, control, secret-on-get, malformed framing, protocol, and host rejection; direct versus exact `gh` argv reconstruction; cleared helper environment; process-group termination on bounds; silent invalid standalone execution; proxy provenance/redaction; missing/unsafe resources; and fixed descriptor mounts in a networkless, Workspace-free Bubblewrap child. No real credential or remote is used.

Phase 5D3D3 proves private stable `gh` backing provenance without credential parsing; direct-helper independence; exact host-plus-port environment; explicit IPv6-literal rejection under the existing remote policy; helper reset/fixed selection across discovery, fetch/history, and push; Git EOF record framing; transport-neutral closed authentication errors; anonymous and SSH credential-proxy absence; and a packaged real Git → proxy → disposable canary-helper exchange. Live credentials and external remotes are forbidden in tests.

Phase 5D4 proves bounded full-file `packed-refs` parsing, ordinary loose-over-packed precedence, packed-only fast-forward publication through an exact loose shadow, byte-identical retained `packed-refs`, complete intent state, and compatibility with the already accepted packed-branch commit/recovery path. It adds no packed-ref rewrite, deletion, maintenance, network, credential, renderer, or MCP authority.

Phase 5B3B1 additionally proves that planning is possible only for the retained native empty-folder capability, that new entries and nested/unsafe/read-only destinations fail before authority, that URL/path/root identity do not cross IPC or Debug, and that selection/cancellation/creation invalidate retained plans. It adds no network or mutation substitute for the remaining Phase 5B3B/5B3C gates.

Phase 5C1 proves bounded commit message/local identity parsing; canonical branch and clean ordinary-index requirements; every operation/conflict marker refusal; valid prior/current managed snapshots; managed create/update/delete with exact unrelated `HEAD` and worktree preservation; SHA-1/SHA-256 candidate parity; filter/hook/attribute/signing/config canaries; private staging bounds; independent object-ID recomputation; race invalidation; redacted plans; and absence of active repository/Workspace writes. Phase 5C2 proves append-only exact pack installation, one operation-bound intent, index-first/direct-ref-last replay/CAS without reflog writes, six metadata and four quarantine interruption boundaries, foreign-state preservation, and fresh managed-effect verification. Phase 5C3 proves one-use pathless native planning/apply/cancel/recovery, exact parented confirmation, off-thread publication, closed renderer payloads, recovery-only status, and accessible wide/narrow renderer states.
| MCP | schemas/errors, aliased launch handles, duplicate/nested roots, per-root capabilities, revisions, prompt injection, plan/approval-child provenance+pipe expiry/replay/substitution, change summary, stdout purity |
| UI | empty/loading/error/conflict, keyboard, focus, announcements, long content, 200% zoom, theme, reduced motion, narrow panes |

## Search relevance and performance

Maintain a checked-in synthetic corpus with adversarial overlaps:

- exact title versus body repetition;
- alias versus title prefix;
- exact command/path/error phrase versus generic prose;
- Category/Tag matches;
- tested/stale/archived tie behavior;
- Unicode/case/punctuation and long tokens.
- bounded plain excerpts, Unicode-scalar highlight ranges, overlap merging, payload rejection, and every status/archived scope;
- production-visible p95 including debounce/render plus a deterministic main-thread transformation budget.

Ranking tests assert ordered IDs and matched-field reasons, not unstable raw floating scores. A deterministic generator produces 100, 1,000, and 5,000-Guide corpora without real user data.

Production benchmark records:

- commit, release profile, corpus seed/size/bytes;
- CPU, RAM, storage/filesystem, distro/WebKit (for UI latency);
- cold build/rebuild, incremental index, warm query p50/p95/p99, visible latency, peak RSS, long main-thread tasks.

Budgets are in `docs/product-spec.md`. Regressions over 10% require investigation even if still under the absolute budget.

## Accessibility and visual verification

Automated checks include semantic role/name assertions, axe-compatible audits where supported, focus order, live regions, contrast tokens, reduced-motion CSS, and no forbidden interaction primitives. They do not replace manual review.

For each major UI gate:

1. Fetch the latest rules required by `.codex/skills/web-design-guidelines/SKILL.md`.
2. Review actual UI source and report terse `file:line` findings.
3. Capture the complete matrix in `docs/design-direction.md` using deterministic fixture Workspaces.
4. Inspect light/dark, target sizes, focus, 200% scaling, long strings/commands, errors/conflicts, and reduced motion.
5. Run keyboard-only and screen-reader smoke passes; record findings and fixes in `PROGRESS.md`.

Screenshot artifacts are stored outside canonical source or in an explicitly documented review-artifact folder; avoid committing large transient images unless they are stable regression fixtures.

## Security tests

- Malicious Markdown/URL/Attachment and Git repositories are checked-in as generators or safe fixtures; never include live credentials/executable malware.
- Secret canaries appear in content, environment, Git remotes, and subprocess stderr; assert absence from UI/log/error/MCP surfaces.
- Race tests repeat enough to exercise swaps but remain bounded; deterministic synchronization hooks are preferred over sleeps.
- Fault injection is explicit at durability stages and validates no silent success.
- Fuzzers have bounded CI smoke corpora and separate extended runs.

## Standard gates

### Every task

- Focused unit/integration tests for changed behavior.
- Formatting, lint, and type/check for touched language.
- Review actual diff and generated files.
- Linux CI must prove a real networkless Bubblewrap launch before running the complete test suite. On disposable Ubuntu 24.04 hosted runners, the workflow enables unprivileged user namespaces, clears the AppArmor user-namespace gate for that boot, and exercises the production `/usr/lib64` loader topology; no Bubblewrap-dependent test is skipped.

### Meaningful phase

- `pnpm check`, `pnpm test`, `pnpm test:integration`, `pnpm build`.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- Contract drift check.
- Production Tauri build/check and native smoke when applicable.
- Applicable performance, visual, accessibility, and security review.

If a command does not yet exist, the phase packet states the equivalent and `PROGRESS.md` records the gap. Missing infrastructure cannot be described as passing.

Phase 6E2A specifically requires exact public-schema, preview/capability, native-approved execution, JSON-RPC, replay, inherited cancellation, and post-plan external-change tests for the complete Category reorder effect set. These run with temporary Workspaces only; production code retains no approval override.

Phase 6E2B adds exact schema, empty/default/nonempty validation, approval/denial/replay, post-plan drift, real JSON-RPC cancellation, recoverable-trash, and deterministic final manifest-race tests for Category removal. No test uses personal Workspace data, and approval overrides remain test-only.

Phase 6E2C adds the exact thirty-two-tool schema, readiness/already-Tested/archived validation, write-without-broad capability behavior, native approval/denial/replay/scope/expiry, post-plan drift, core journal-boundary cancellation, deterministic real JSON-RPC cancellation paused before journal creation, and verification-hash/effective-status assertions. Test approval outcomes and the boundary probe remain test-only.

Phase 6E2D1 adds the exact thirty-three-tool schema plus Attachment-title validation, unknown-metadata preservation, immutable path/hash/size/media-type/ID assertions, Tested-to-Stale behavior, read-only refusal, stale revision, missing ID, and byte-identical no-op coverage.

Phase 6E2D2 expands the exact schema to thirty-five tools and covers capability-only previews, native denial, single-use replay, scope/expiry/archive/missing/drift refusal, exact recoverable trash bytes, unknown-metadata/trust behavior inherited from core, and deterministic real-protocol cancellation at the final pre-journal check.

Phase 6E2D3 expands the exact schema to forty-five tools after Phase 6E3B. It covers exact Guide-directory confinement, declared/unsafe/oversize filtering, bounded pagination/cache/expiry, token replay/scope/eviction, no path/hash/byte disclosure, exact file identity/content recheck, stale/archived/limit/duplicate refusal, unknown preservation, Tested-to-Stale, and replacement/hash races.

Phase 6E3A expands the exact schema to thirty-nine tools. Focused coverage binds opaque `gitState`, startup write+Git-write authority, normalized messages, closed plan/apply/recovery inputs, single-use approval, unrelated-file preservation, managed-only publication, no-recovery refusal, non-disclosure, atomic per-Workspace normal/recovery exclusion across planning and active execution, exact interrupted-operation recovery binding, and real JSON-RPC cancellation at the final pre-intent boundary. The accepted core commit/recovery adversarial suite remains mandatory. The complete gate and Security Reviewer acceptance remain mandatory before checkpoint.

Phase 6E3B expands the exact schema to forty-three tools. Focused coverage requires the closed pull/push direction matrix, inspection denial before network, real JSON-RPC cancellation propagation to the core network boundary, shared commit/sync lifecycle exclusion, safe schema/non-disclosure checks, and exact interrupted-pull recovery binding. The already accepted core HTTPS/SSH inspection, classification, fast-forward publication, push postflight, authentication, hostile remote, and recovery suites remain mandatory; the final repository gate and Security Reviewer acceptance are required before checkpoint.

## Test result recording

`PROGRESS.md` records date, exact command, pass/fail counts or concise result, relevant environment, known flakes/failures, and commit/tree state. The latest verified commit means all claimed gates ran against that commit; if docs changed afterward, say so explicitly.
