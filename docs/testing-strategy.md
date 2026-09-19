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

### End-to-end desktop tests

Use the most stable Tauri/WebDriver-compatible automation available at Phase 2 and retain manual smoke coverage where Linux WebKit automation is incomplete. Tests launch the production-like native app against a temporary Workspace and exercise open/create/search/read/edit/restart/conflict. Never rely exclusively on the Vite browser view because native IPC, CSP, WebKitGTK, window behavior, and file dialogs are product behavior.

## Required coverage matrix

| Area | Minimum cases |
| --- | --- |
| Detection | nonexistent, empty, current, supported old, newer, Markdown collection, unrelated, non-writable, malformed, Git, nested Git |
| Initialization | changed-after-preview, collision, partial failure, permissions, existing files, deterministic manifest |
| Import/adoption | safe Markdown, frontmatter variants, source unchanged, symlink/special file, collision, oversize, rollback |
| Compatibility/migration | each supported version, unknown fields, backup validation, interrupted/repeated migration, newer refusal |
| Markdown | known/unknown fields, comments/order strategy, Unicode/BOM/CRLF, duplicate keys, aliases/tags, deep/large input, reserved sections, malicious HTML/links |
| Category/Guide | every lifecycle/status/move/rename/reorder/archive/restore/delete path and invalid transition |
| Tested trust | AI/default draft, missing/false/wrong confirmation, explicit success, stale→tested, no implicit transitions |
| Search | field extraction, exact title/alias/command/path/error order, typo/prefix policy, archived scope, malformed docs, incremental/rebuild/corrupt index |
| External/concurrent | atomic-save event sequences, dirty draft, rapid rename, watch overflow/loss, path+byte revision, cooperating-writer winner, external/held-inode race with displaced bytes retained |
| Atomic/recovery | injected failure before/after object/journal/temp/exchange/directory/commit sync; all-old/all-new/mixed/neither; hostile journal |
| Confinement | absolute/parent/NUL/Unicode tricks, symlink each component/final, race swap, hardlinks, special files, non-UTF-8 names |
| Git | unavailable, clean/dirty/untracked, branch/detached, remotes/redaction, ahead/behind/diverged, merge/rebase conflict, unsafe owner, nested repo, auth/offline, timeout/output cap, hostile hook/filter/fsmonitor/textconv/pager/env |
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

### Meaningful phase

- `pnpm check`, `pnpm test`, `pnpm test:integration`, `pnpm build`.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- Contract drift check.
- Production Tauri build/check and native smoke when applicable.
- Applicable performance, visual, accessibility, and security review.

If a command does not yet exist, the phase packet states the equivalent and `PROGRESS.md` records the gap. Missing infrastructure cannot be described as passing.

## Test result recording

`PROGRESS.md` records date, exact command, pass/fail counts or concise result, relevant environment, known flakes/failures, and commit/tree state. The latest verified commit means all claimed gates ran against that commit; if docs changed afterward, say so explicitly.
