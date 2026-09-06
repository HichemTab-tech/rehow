# Task Packet: Phase 1C bounded YAML parser and reconciliation

- **Objective:** Implement the accepted Phase 1C generic YAML source pipeline in
  `rehow-core`: bounded whole-YAML and Guide-envelope scanning, exhaustive
  read-only `yaml-edit`/rowan presentation inspection, bounded incremental
  Saphyr event handling, exact YAML 1.2.2 core scalar resolution into the
  Phase 1B builder, and an owned immutable reconciliation/comment artifact. A
  successful result distinguishes malformed input from semantic-readable but
  non-round-trippable input, retains accepted source bytes exactly once, and
  supplies no product-field, filesystem, protocol, serialization, or mutation
  authority.
- **Required reasoning level:** low. Use the project Worker role
  (`gpt-5.6-sol`, low) exactly. ADR 0013 freezes the module/API shape,
  dependency calls, state machines, meters, precedence, artifact, comment
  ownership, test seams, and evidence. The Worker implements those decisions
  literally and must not simplify the dual-parser design, select alternate
  APIs, broaden visibility, or infer later-phase behavior.
- **Prerequisites:**
  - Signed Phase 1B implementation checkpoint `df067b2` and acceptance ledger
    `43212be` are present. Signed ADR 0013 checkpoint `f39d7ab` and its ledger
    `1127d09` are present. `git verify-commit df067b2` and
    `git verify-commit f39d7ab` must pass before editing.
  - `PROGRESS.md` must name this packet's later reviewed signed checkpoint
    before implementation dispatch. Independent Reviewer/high and Security
    Reviewer/xhigh must first approve the packet with no blocker/high/medium
    finding.
  - Read `AGENTS.md`, `PROGRESS.md`, this packet, all specifications/ADRs listed
    below, `docs/evidence/phase-1b-protocol-domain.md`, current manifests and
    lockfiles, and the actual Phase 1B domain implementation/tests. Run
    `git status --short --branch` and `git log --oneline -10`; stop if an
    allowed path has another writer or differs from the reviewed packet base.
  - The unrelated untracked root executable `x` is not Rehow state. Never
    execute, inspect, read, modify, stage, delete, hash, or include it in any
    command whose output becomes evidence.
  - Use installed Node 26.2.0, pnpm 11.20.0, and Rust/Cargo 1.98.0 unchanged.
    Do not install or switch Node or pnpm. Do not install an audit, fuzz,
    snapshot, parser, or other development tool merely for this packet.
  - Before changing manifests, record the baseline SHA-256 of
    `Cargo.lock` and `packages/contracts/src/generated.ts`, then run the locked
    release build and record the exact newest `rehow-core` rlib,
    `target/release/rehow-mcp` binary, and `target/release/rehow-desktop` binary
    sizes in the dependency evidence. In one retained shell create a private
    temporary evidence directory with `phase1c_tree_dir="$(mktemp -d)"` and
    capture every exact baseline Cargo tree listed under Commands before the
    manifest edit. If an artifact or tree cannot be identified/captured
    deterministically, escalate before editing rather than guessing. This
    required build measures the unchanged signed baseline; the post-manifest
    dependency audit below must finish before compiling or executing the changed
    dependency graph or any newly resolved dependency/project change.
- **Allowed files or subsystem:** The Worker has exclusive write ownership only
  of:
  - `crates/rehow-core/Cargo.toml` and root `Cargo.lock`;
  - `crates/rehow-core/src/lib.rs`, only to add the exact crate-private
    `parsing` module declaration and lint allowance from ADR 0013;
  - `crates/rehow-core/src/parsing/mod.rs`, `source.rs`, `presentation.rs`,
    `semantic.rs`, `scalar.rs`, and `reconcile.rs`;
  - private unit-test modules beneath `crates/rehow-core/src/parsing/tests/`,
    including only narrowly named Rust test modules and byte fixtures beneath
    `fixtures/accepted/`, `fixtures/malformed/`, and `fixtures/read-only/`;
  - `crates/rehow-core/src/domain.rs` and
    `crates/rehow-core/src/domain/yaml_value.rs`, only for ADR 0013's exact
    `#[cfg(test)]` `UnknownYamlTestMeter` re-export and
    `UnknownYamlDocumentBuilder::set_test_meter` bridge plus its tests;
  - `crates/rehow-core/tests/public_api.rs` and new matching files under
    `crates/rehow-core/tests/ui/` for the six exact Phase 1C trybuild negatives
    and their generated `.stderr` baselines;
  - `docs/evidence/phase-1c-dependencies.md` and
    `docs/evidence/phase-1c-parser-matrix.md`.

  `packages/contracts/src/generated.ts` is read-only and must remain
  byte-identical. Other than the explicitly allowed
  `crates/rehow-core/Cargo.toml` and root `Cargo.lock`, do not edit any manifest
  or lockfile. Do not edit any contract generator,
  adapter, UI, Tauri capability, MCP code, other core module, existing Phase 1B
  test/fixture except the named public-API harness, specification, ADR, agent
  configuration, script, `PROGRESS.md`, or task packet. Do not stage or commit;
  the Orchestrator owns both.
- **Relevant specifications and ADRs:**
  - `AGENTS.md`: canonical-content, hostile-input, no-execution, Rust-core,
    testing, orchestration, model, and commit invariants.
  - `docs/implementation-plan.md`: Phase 1 milestone 3 and Phase 1 gate; Phase
    1C/1D/1E authority split and dependency policy.
  - `docs/architecture.md`: core boundary, generic semantic/presentation split,
    disclosure rules, and parser/security review triggers.
  - `docs/content-model.md`: 256 KiB YAML, 2 MiB Guide, depth/node/entry/scalar
    limits; unknown YAML semantics; exact source preservation; Guide envelope.
  - `docs/testing-strategy.md`: YAML semantic/presentation matrix, hostile and
    limit cases, standard gates, evidence recording, and temporary-data rules.
  - `docs/threat-model.md`: hostile YAML, exhaustion, dependency unsafe,
    disclosure, and parser security gates.
  - `docs/error-contract.md`: diagnostic spellings and the rule that Phase 1C
    does not directly construct protocol errors.
  - ADR 0011,
    `docs/adr/0011-bounded-yaml-semantics-and-lossless-span-edits.md`: dual
    parser policy, exact content subset, semantic/numeric/key-work accounting,
    source preservation, and Phase 1C boundary.
  - ADR 0012,
    `docs/adr/0012-phase-1b-protocol-construction-and-dependencies.md`: the
    accepted Phase 1B `UnknownYamlDocumentBuilder`, canonical scalar types,
    limits, errors, traits, and private construction authority.
  - ADR 0013,
    `docs/adr/0013-phase-1c-parser-and-reconciliation-contract.md`: normative
    for every exact API, field, trait, call allowlist, state transition,
    counter, precedence rule, fixture class, and test control abbreviated by
    this packet. If this packet and ADR 0013 appear to conflict, stop and ask
    the Orchestrator; do not choose one silently.
- **Exact required behavior:** Implement in this order so each boundary can be
  inspected before the next one is trusted:
  1. Record baseline evidence, then add only these direct runtime edges to
     `rehow-core`; retain every existing dependency and feature unchanged and
     add no dev dependency:

     ```toml
     rowan = "=0.16.1"
     saphyr-parser = "=0.0.12"
     yaml-edit = { version = "=0.3.1", default-features = false }
     ```

     Enable no rowan or Saphyr feature. `saphyr-parser/debug_prints` and
     `yaml-edit/binary` must be absent. After editing the manifest, run exactly
     one unlocked `cargo fetch`. This performs dependency resolution/download
     and updates `Cargo.lock` without compiling or executing a build script or
     procedural macro. Do not run `cargo check`, `build`, `test`, `clippy`, or
     rustdoc against the changed graph yet. Immediately inspect the manifest/
     lock delta. Preserve set A, the exact full lock identities
     `(name, version, source, checksum)` newly added by that reviewed diff;
     inspect their exact local sources whether fetched now or already cached.
     The lock may add only identities required by these three edges and may not
     change any existing identity/version/source/checksum.

     Capture the matching post-resolution locked trees for every baseline
     configuration, then mechanically `diff -u` each baseline/post pair. From
     those diffs derive set B, every already-locked package identity newly
     reachable from one of the three direct edges or from any configuration of
     the changed `rehow-core`/Workspace graph, and set C, every already-locked
     package identity whose enabled feature set changes in any captured
     configuration. The precompile changed-graph audit set is exactly
     `A union B union C`. Exclude packages merely downloaded for another
     target/graph when they are in none of A, B, or C; unrelated downloads do
     not widen or satisfy the set.

     For every union member, record full lock identity and duplicate-version
     facts; exact local source/revision; license; declared MSRV; all unsafe
     occurrences and their enabled-feature reachability; build-script and
     proc-macro presence/behavior under the features/configurations that will be
     compiled; yank state; and current RustSec/OSV finding. Only after every
     tree diff and union member has no unresolved discrepancy may the Worker
     compile or execute the changed dependency graph, beginning with
     `cargo check -p rehow-core --locked`. Any unrelated lock
     movement, alternate pin, unexplained reachability/feature delta, yanked
     package, advisory, license/MSRV/source/unsafe/build-script/proc-macro
     discrepancy, unexpected feature-gated behavior, or unavailable evidence is
     escalation before compiling/executing the changed graph or any newly
     resolved dependency/project change. This does not prohibit the required
     pre-edit signed-baseline release build.
  2. Add to `lib.rs` exactly:

     ```rust
     #[allow(dead_code, reason = "Phase 1C parser is consumed beginning in Phase 1D")]
     pub(crate) mod parsing;
     ```

     Do not add any item-level allowance or other lint allowance. The six
     production child files and private/re-export visibility are exactly ADR
     0013. `presentation`, `semantic`, `scalar`, and `reconcile` stay private;
     there is no public parser trait, configurable parser, dependency wrapper,
     or generic source-kind input.
  3. Expose to core siblings only the two exact production entries
     `parse_yaml_document(Box<[u8]>) -> Result<ParsedYamlDocument, ParseFailure>`
     and `parse_guide_source(Box<[u8]>) -> Result<ParsedGuideSource,
     ParseFailure>`, plus every `pub(crate)` type/accessor enumerated in ADR
     0013. Preserve the exact private fields, constructors, traits, absent
     traits, and no-re-export rule for coordinate, envelope, ID, vocabulary,
     artifact, node, entry, comment, round-trip, and failure types. IDs index
     boxed slices; root is private `NodeId(0)`. Retain the accepted input box
     exactly once, including Guide body and comment bytes. Raw accepted content
     is reachable only through `source_bytes`, `yaml_bytes`, `body_bytes`, and
     checked ranges returned by the prescribed crate-private accessors; comment
     records copy no text. Retain no second raw-source/body/comment allocation,
     dependency handle, marker/span, `Cow`, rowan object, line table, or
     temporary trace. The separately bounded decoded/canonical semantic strings
     required by ADR 0013 are not raw-source copies.
  4. Implement the fixed-size, allocation-free byte preflight and exact
     `WholeYamlEnvelope`/`GuideEnvelope` behavior from ADR 0013. Check 262,144
     whole-YAML bytes and 2,097,152 Guide bytes before UTF-8 or parser
     allocation. Handle only the optional byte-zero UTF-8 BOM; reject raw BOM
     in the YAML/envelope region, bare CR, and mixed YAML line endings. For a
     Guide require exact opening `---`, the first column-zero exact closing
     `---`, one same-style blank separator, and a byte-zero-through-separator
     cap of 262,144. Distinguish frontmatter overflow, missing close, and
     missing separator exactly. Keep body bytes opaque; body line endings and
     raw U+FEFF are not YAML failures. Produce checked u32 file/YAML ranges,
     final-newline facts, and one-based positions only after full UTF-8
     validity; `InvalidUtf8` always has no position.
  5. Run `YamlFile::parse` only on the bounded UTF-8 parser slice. Use exactly
     the yaml-edit/rowan call allowlist in ADR 0013. Select positioned errors by
     valid range start/end, fixed kind rank, then original index; invalid
     ranges never shadow a valid range and yield no position when all are
     invalid. Walk iteratively and exhaustively match all 12 node and all 50
     listed lexical/structural `SyntaxKind` cases in both element arms without
     a wildcard. Enforce the exact normalized CST contexts, semantic wrapper
     arity, zero-width implicit-empty form, scalar raw cap/style derivation,
     source-only directive/marker/anchor/alias/tag/BOM rules, literal tag
     allowlist, merge candidates, token-text/source equality, ordered complete
     leaf coverage, optional final EOF, and root-text no-op check. Any new
     `SyntaxKind` must cause compilation failure until reviewed.
  6. Build the source candidate trace, `SourceGuard`, and exact 16-byte/alignment
     `TailDescriptor` from ADR 0013 once per presentation semantic node. Enforce
     `2 * yaml_len + 1` descriptors with checked arithmetic and no text/range or
     artifact ID in the bounded tail. Explicit built-in tags and exact
     untagged plain mapping-key `<<` are the only non-none guards. Never use a
     decoded string to establish a source guard.
  7. Consume Saphyr only through `Parser::new_from_str`,
     `keep_tags(false)`, and bounded `next_event`; never use `load`, `peek`,
     custom input/receiver/scanner, marker, or span. Enforce the exact stream
     sequence and iterative collection/key-value state machine. Drain at most
     `8 * yaml_len + 32` calls, retaining at most 40,004 owned trace records.
     A scan failure ends dependency calls but outranks later semantic failures.
     Use the exact temporary decoded/raw aggregate bounds and checked
     allocation arithmetic in ADR 0013.
  8. Compute every `EventGuard` and compare source/event guards only through
     the proven ordinal prefix. After the first tuple disagreement, never
     resynchronize or associate later ordinals; continue scanning both guard
     streams once. Any proved later non-none guard is
     `ParserTagDisagreement`. Implement the exact borrowed-text counter,
     `Uninspectable` state, fixed-length present-tag inspection, permanent
     no-text-read/no-scan behavior after `TailBorrowedTextInvariant`, linear
     tail-work formula, and precedence exceptions. Never guess whether an
     uninspectable plain mapping key is a merge.
  9. After a successful bounded drain and source/event policy checks, resolve
     scalar text using the literal ordered YAML 1.2.2 ASCII grammars in ADR
     0013. Do not use regex, bigint, yaml-edit scalar classification, `f64`, or
     machine-width whole-number conversion. Implement decimal normalization,
     left-to-right octal/hex multiply-add, finite float coefficient/exponent
     normalization, infinities, NaN, signed-zero rules, and explicit-tag row
     selection exactly. Apply checked `S`, `C`, `R`, pre-conversion and result
     charges, 16 Mi numeric-work budget, 80 Ki aggregate digit budget, and
     65,536-digit canonical-scalar cap in the mandated same-event order.
  10. Fold accepted events one-for-one through a fresh Phase 1B
      `UnknownYamlDocumentBuilder`; it remains semantic/duplicate/key-work
      authority. Mirror only the counters ADR 0013 names and map every builder
      kind exactly. A mirror/builder mismatch, poison, impossible accounting,
      or event-call/tail invariant returns `SemanticBuilderInvariant`; no
      partial tree or trace survives. Finish only the one closed implicit
      document with a mapping root and string top-level keys.
  11. Reconcile the two Rehow-owned traces by a single preorder zip, comparing
      the eight exact dimensions in ADR 0013 order. Create exact immutable
      `ReconciledNode`/`ReconciledEntry` records, semantic parent/role/children,
      concrete/tag/payload ranges, and the special no-range implicit-empty
      site. Rebase yaml offsets to file offsets once with checked arithmetic;
      validate UTF-8 boundaries, containment, ancestor-only nesting, and
      sibling/entry disjointness. At the first structural mismatch emit one
      issue, mark the suffix `Unreconciled`, retain only the proven contiguous
      entry prefix, and never invent a later ID or correspondence.
  12. Classify comments with the exact source-linear algorithm: bounded line
      starts and blank flags cover the complete bounded region for whole YAML,
      or byte zero through the separator line for a Guide; one iterative CST
      enter/leave traversal; checked `8 * yaml_len + 64` visit guard; two
      prefix/counting passes; then one monotonic entry/comment merge per parent.
      Implement same-line, scalar-nested, block boundary, document, flow, and
      ambiguous ownership rules exactly. Keep only ranges/placement/owner, no
      copied comment text. Enforce the test work counter
      `<= 12*S + 8*T + 12*N + 8*E + 8*C + 64` with checked arithmetic. An
      unresolved owner adds `CommentOwnershipAmbiguous` but does not make the
      parse malformed.
  13. Return `RoundTripStatus::Proven` only after correspondence, all ranges,
      comment ownership, and exact no-op source rendering prove safe. Only an
      otherwise fully accepted, unguarded structural/presentation/
      correspondence disagreement may return the complete semantic artifact as
      `ReadOnly`. A guarded tag/merge disagreement, either parser failure,
      source/semantic restriction, resource failure, or invariant poisoning is
      always sanitized `Err(ParseFailure)`, never `ReadOnly`. Retain the first
      32 read-only issues and exact checked `u32` total in ADR 0013's detection/
      reason order. Populate only the permitted node/entry/comment identifier
      and lowest safe YAML offset. Implement every ten-reason mapping and the
      exact malformed/read-only split; a structural mismatch creates one suffix
      issue, not fabricated per-node issues.
  14. Implement `ParseFailure`, all 41 exact `ParseFailureKind` variants, all
      14 `ParseLimitScope` variants, `ParseLimitFacts`, their exact traits and
      accessors, fixed display `invalid Rehow YAML source`, absent source chain,
      and non-content Debug. Rejected content may exist only in bounded
      temporary parsing allocations that are dropped before `Err`; no rejected
      byte/text, parser message/code, body/comment content, path, dependency
      object, or dependency marker may enter `ParseFailure`, logs, evidence, or
      another retained copy. Limit facts exist only as
      ADR 0013 specifies, with the fixed accepted maximum and first refused or
      checked-lower-bound value. Apply the global 14-stage malformed precedence
      and all within-stage tie rules exactly. Phase 1C does not construct
      `DiagnosticCode`, `ErrorDto`, `RequestId`, or `SafeRelativePath`; merely
      test/document ADR 0013's exhaustive later diagnostic mapping.
  15. Add only the exact `#[cfg(test)]` Phase 1B meter bridge in ADR 0013. It is
      crate-test-only, directly sets one active `usize` meter, cannot change a
      limit or skip a charge, and returns `Poisoned` without mutation for a
      poisoned builder. Add private, unnameable parsing controls/reducers for
      exactly the listed copied-record, range, guard, stream, borrowed-text,
      numeric, and mirror faults. They alter only Rehow temporary records after
      dependency calls; no feature/env/global/trait-object hook or artifact
      constructor exists. Normal builds and external crates must not see them.
  16. Keep production Rehow parsing code free of `unsafe`; retain no parser
      object after return and perform no I/O, logging, clock, entropy, hashing,
      network, path handling, Markdown parsing, serialization, or mutation.
      Regenerate contracts only as a verification action: generated TypeScript
      must have its exact baseline SHA and contain none of the parser/artifact,
      coordinate/comment/round-trip, or `UnknownYaml*` names from ADR 0013.
- **Important states and edge cases:**
  - Empty whole YAML, empty Guide YAML, empty Guide body, no-newline whole YAML,
    LF/CRLF, byte-zero BOM, final/no-final newline, blank-leading Guide body,
    multibyte scalars before and inside ranges, and every exact envelope cap.
  - Invalid UTF-8 at the start, after LF/CRLF, invalid continuation, and
    incomplete final sequence; raw BOM at every YAML/envelope location; bare
    CR/mixed newlines; missing/late/indented delimiters and missing separator.
  - Every CST node/token kind, positioned-error ordering/ties/invalid ranges,
    coverage gaps/overlaps/text mismatch/extra zero-width tokens, no-op tree
    mismatch, and implicit omitted key/value/item versus explicit null/empty.
  - All directives/document markers, anchors/aliases on every node kind,
    local/verbatim/custom/remapped tags and tag/anchor orderings; all seven
    accepted literal tags with correct and incorrect node/value; merge forms
    including quoted/tagged string `<<`.
  - Exact/one-over depth 16, map 256, sequence 2,048, nodes 20,000, raw/decoded/
    canonical scalar 65,536, numeric work 16 Mi, numeric digits 80 Ki,
    semantic encoding 8 MiB, and semantic comparison 32 MiB. Equality is
    accepted and the next complete charge fails; overflow never wraps.
  - Alternate decimal/octal/hex and float spellings, huge values, carry/
    borrow/cancellation, zeros/signs/exponents, infinity/NaN, YAML 1.1 legacy
    forms as strings, timestamp-like strings, complex semantic keys, duplicate
    spellings, mapping-order and sequence-order distinctions.
  - Every adjacent malformed-precedence stage 5–14, later scan failure after an
    earlier root/limit/numeric candidate, valid and invalid guarded nodes after
    every earlier semantic limit, all shifted/asymmetric tag/merge cases before
    and after node 20,000, and every named borrowed-tail closure test.
  - First disagreement at a root child, mapping key/value, nested collection,
    arity, entry wrapper, scalar style/decoder/raw source, range, overlap,
    rebase, unsupported presentation, no-op render, and comment owner. Prove no
    post-disagreement association or edit authority.
  - Dense comments/newlines at the 256 KiB region, deepest nesting, block/flow
    mixtures, inline/leading/trailing/inter-entry/nested/document/unresolved
    comments, blank-line boundaries, and linear-work exact counter behavior.
- **Security constraints:**
  - Treat every byte, dependency event, syntax node/range, scalar, tag, comment,
    and filename-free diagnostic fact as hostile. Check sizes and arithmetic
    before allocation, decoding, conversion, indexing, copying, or rebasing.
  - Both parsers and Rehow policy must accept before semantic exposure. Only an
    otherwise accepted, unguarded presentation/correspondence disagreement can
    reduce the artifact to read-only. A guarded tag/merge disagreement, parser
    failure, or invariant-poisoned state is sanitized `Err`; no disagreement
    can grant write authority or make malformed content valid.
  - Never use Saphyr markers/spans or dependency diagnostics for coordinates or
    output. Accepted raw source, including body/comment bytes, exists exactly
    once and is reachable only through prescribed crate-private accessors and
    checked ranges; `CommentRecord` copies no text. Rejected content and its
    bounded temporary copies are dropped before return and never enter a
    failure, log, evidence file, parser error, Debug event output, or extra
    retained copy. Never retain or disclose absolute paths or dependency
    objects.
  - Reject directives, aliases, anchors, merge semantics, nonliteral/custom
    tags, duplicate semantic keys, noncharacters, resource-limit excess, and
    incomplete coverage. U+FEFF is valid YAML string data only when escaped.
  - All Rehow traversal/folding/reconciliation is iterative and bounded.
    Standard sort/hash/all-pairs matching, recursive rowan walking, repeated
    parent-chain lookup, rescanning per node/comment, unmetered radix/big-number
    conversion, and post-disagreement resynchronization are forbidden.
  - Use only ADR 0013's dependency call allowlists. No dependency feature
    expansion, unsafe Rehow code, serializer/mutator/green-tree/path/file API,
    or parser fallback is allowed. Dependency panic, advisory, yanking, unsafe
    surface drift, or unbounded behavior blocks acceptance.
  - After the manifest changes, do not compile or execute the changed graph or
    any newly resolved dependency/project change until the exact A/B/C union is
    derived from the reviewed full-lock-identity delta plus mechanical baseline/
    post reachability-feature tree diffs, and every union member has passed the
    cached-or-fetched local-source and security audit for the configurations to
    be compiled. Unrelated Cargo downloads outside A/B/C neither satisfy nor
    expand its evidence. The pre-edit release build is permitted solely to
    measure the unchanged signed baseline.
  - Test controls must be absent outside `cfg(test)` and cannot bypass the
    Phase 1B builder, lower production limits, inject source fragments, forge a
    returned artifact, or expose a public symbol.
- **Tests to add or update:**
  - Add private test modules under `src/parsing/tests/` grouped as
    `source`, `presentation`, `semantic`, `scalar`, `reconcile`, `comments`,
    `precedence`, `limits`, `faults`, `properties`, and
    `dependency_surface`; wire them only beneath `#[cfg(test)]`.
  - Populate the three exact fixture roots from ADR 0013. Accepted fixtures
    cover both entry APIs, LF/CRLF/BOM/final-newline states, every style/tag/
    scalar/collection, complex values, comments, implicit empties, nesting, and
    natural exact limits where the reachability table permits them. Malformed
    fixtures provide one deterministic natural byte case for every
    source-reachable failure and exact/one-over source for each source-reachable
    production limit. Semantic encoding/comparison and every other invariant-
    only limit use only the exact direct reducers, fault controls, and Phase 1B
    test-meter bridge; do not claim a natural parser-slice witness. Read-only
    fixtures plus private controls cover all ten reasons.
    Differential and invariant-only outcomes must remain direct/fault evidence,
    never mislabeled natural fixtures.
  - Assert all 41 failure kinds belong to exactly one reachability class and
    map to a named test; assert exact limit scope/value/observed facts and no
    content-bearing error/debug/source chain. Test every builder-kind mapping
    and every later diagnostic grouping without constructing protocol errors.
  - Implement every ADR 0011/0013 adversarial scalar, tag, duplicate, work,
    precedence, correspondence, comment, and range case, including the 14
    stable guarded-tail test names verbatim. Include exact/one-over and overflow
    cases for all four builder meters and poisoned follow-up using the test-only
    bridge.
  - Run the fixed-seed dependency-free xorshift64 harness for at least 4,096
    arbitrary byte cases and 4,096 mutations of accepted fixtures. Assert no
    panic/bounded completion; every accepted result satisfies semantic
    traversal, source/range/ID/parent/entry invariants. Record seeds and counts.
  - Assert the portable final allocation/count formulas, `TailDescriptor` size
    16/alignment 4, descriptor/trace/line-table/comment/work bounds, and
    permanent text-scan disablement. Record the actual target-specific
    `size_of` values for artifact record types as observed evidence only; do not
    invent or assert portable layout constants for those records.
  - Extend the existing `public_api` trybuild harness with exactly these six
    negative sources and reviewed `.stderr` files:
    `phase1c_parser_module_is_private.rs`,
    `phase1c_parse_entries_are_private.rs`,
    `phase1c_artifact_fields_are_private.rs`,
    `phase1c_coordinate_constructors_are_private.rs`,
    `phase1c_round_trip_report_cannot_be_forged.rs`, and
    `phase1c_test_controls_absent.rs`. The existing positive fixture remains
    byte-for-byte Phase 1B API behavior; do not import a parser symbol.
  - Compile one private adapter fixture that calls every allowed dependency API
    and no other one. Add a source audit test over production parsing imports/
    calls for ADR 0013's forbidden catalogs and exact dead-code reason. Run
    rustdoc and assert no parsing/artifact/test-control symbol appears publicly.
  - Run contract generation/check, assert the generated file SHA equals the
    baseline, and search for every forbidden parser/unknown-YAML TS name.
- **Commands to run:** Use this ordered gate and record exact results/counts in
  the two Phase 1C evidence files. Do not omit a failure or call the phase green
  when any command fails.
  1. Before editing: `node --version`, `pnpm --version`, `rustc --version`,
     `cargo --version`, both `git verify-commit` commands, baseline SHA/size
     recording, and `cargo build --release --workspace --locked`. Confirm
     `rustc -vV` reports the reviewed host `x86_64-unknown-linux-gnu`; a changed
     host requires the Orchestrator to freeze the replacement relevant-target
     tree command before editing. In one retained shell run
     `phase1c_tree_dir="$(mktemp -d)"`, then capture these exact baseline files:

     ```text
     cargo tree --workspace --locked -e normal > "$phase1c_tree_dir/baseline-workspace-normal.txt"
     cargo tree --workspace --locked -e features > "$phase1c_tree_dir/baseline-workspace-default-features.txt"
     cargo tree -p rehow-core --locked -e features > "$phase1c_tree_dir/baseline-core-default-features.txt"
     cargo tree -p rehow-core --all-features --locked -e features > "$phase1c_tree_dir/baseline-core-all-features.txt"
     cargo tree --workspace --all-features --locked -e features > "$phase1c_tree_dir/baseline-workspace-all-features.txt"
     cargo tree --workspace --all-features --target x86_64-unknown-linux-gnu --locked -e features > "$phase1c_tree_dir/baseline-workspace-all-features-linux-host.txt"
     ```

     The baseline sizes are the exact newest `rehow-core` rlib,
     `target/release/rehow-mcp` binary, and
     `target/release/rehow-desktop` binary, never the desktop rlib.
  2. After the manifest edit, run the single non-executing unlocked resolution/
     download command `cargo fetch`. It may write the resolved `Cargo.lock` and
     Cargo download cache but does not compile or execute build scripts or
     procedural macros. Immediately run
     `git diff -- Cargo.lock crates/rehow-core/Cargo.toml`, preserve exact set A
     from that output, and capture the matching post-resolution configurations:

     ```text
     cargo tree --workspace --locked -e normal > "$phase1c_tree_dir/post-workspace-normal.txt"
     cargo tree --workspace --locked -e features > "$phase1c_tree_dir/post-workspace-default-features.txt"
     cargo tree -p rehow-core --locked -e features > "$phase1c_tree_dir/post-core-default-features.txt"
     cargo tree -p rehow-core --all-features --locked -e features > "$phase1c_tree_dir/post-core-all-features.txt"
     cargo tree --workspace --all-features --locked -e features > "$phase1c_tree_dir/post-workspace-all-features.txt"
     cargo tree --workspace --all-features --target x86_64-unknown-linux-gnu --locked -e features > "$phase1c_tree_dir/post-workspace-all-features-linux-host.txt"
     cargo tree --workspace --locked -e features -i saphyr-parser@0.0.12
     cargo tree --workspace --locked -e features -i yaml-edit@0.3.1
     cargo tree --workspace --locked -e features -i rowan@0.16.1
     ```

     Mechanically run these six comparisons, retain their outputs in the
     temporary directory, and record each observed exit code in dependency
     evidence:

     ```text
     diff -u "$phase1c_tree_dir/baseline-workspace-normal.txt" "$phase1c_tree_dir/post-workspace-normal.txt" > "$phase1c_tree_dir/diff-workspace-normal.txt"
     diff -u "$phase1c_tree_dir/baseline-workspace-default-features.txt" "$phase1c_tree_dir/post-workspace-default-features.txt" > "$phase1c_tree_dir/diff-workspace-default-features.txt"
     diff -u "$phase1c_tree_dir/baseline-core-default-features.txt" "$phase1c_tree_dir/post-core-default-features.txt" > "$phase1c_tree_dir/diff-core-default-features.txt"
     diff -u "$phase1c_tree_dir/baseline-core-all-features.txt" "$phase1c_tree_dir/post-core-all-features.txt" > "$phase1c_tree_dir/diff-core-all-features.txt"
     diff -u "$phase1c_tree_dir/baseline-workspace-all-features.txt" "$phase1c_tree_dir/post-workspace-all-features.txt" > "$phase1c_tree_dir/diff-workspace-all-features.txt"
     diff -u "$phase1c_tree_dir/baseline-workspace-all-features-linux-host.txt" "$phase1c_tree_dir/post-workspace-all-features-linux-host.txt" > "$phase1c_tree_dir/diff-workspace-all-features-linux-host.txt"
     ```

     Exit 0 means unchanged, 1 means differences to classify into B/C, and 2
     is escalation. Record all six diffs in dependency evidence and derive the
     exact A/B/C union before source review. Inspect the exact local registry/
     Git source of every union member whether cached or fetched, and exclude
     unrelated downloaded packages outside the union. For each member record
     exact lock identity/checksum/source, revision, license, MSRV, all unsafe
     plus feature reachability, build scripts, proc-macro targets/dependencies,
     enabled-feature deltas, duplicates, yank state, and advisories. Check
     `command -v cargo-audit` and
     `command -v osv-scanner` without installing either; use an already
     available tool according to its installed-version help and query official
     RustSec and OSV records for every member of the exact A/B/C union.
     Record dated sources and all limitations. If network, source, registry
     metadata, or an advisory check is unavailable, stop before compiling or
     executing the changed graph and escalate; do not describe missing evidence
     as clean. Only after the manifest/lock/source/feature/security review has
     no unresolved discrepancy run
     `cargo check -p rehow-core --locked`.
  3. Focused implementation loop after that dependency gate:
     `cargo test -p rehow-core --lib parsing --locked`,
     `cargo test -p rehow-core --test public_api --locked`,
     `cargo fmt --all --check`, and
     `cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings`.
  4. Contract/API gate: record the generated TS SHA, run
     `pnpm contracts:generate`, compare the SHA and byte diff to baseline, run
     `pnpm contracts:check`, the explicit forbidden-name search from ADR 0013,
     `cargo doc -p rehow-core --no-deps --locked`, and inspect/search generated
     rustdoc for absence of the private parser and test controls.
  5. Full gate: `pnpm install --frozen-lockfile`, `pnpm check`, `pnpm test`,
     `pnpm test:integration`, `pnpm build`, `cargo fmt --all --check`,
     `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`,
     `cargo test --workspace --all-features --locked`,
     `cargo test -p rehow-core --test public_api --locked`,
     `cargo check -p rehow-core --lib --locked`,
     `cargo build --release --workspace --locked`,
     `pnpm tauri:build`, `python3 scripts/check_bootstrap.py`, and
     `git diff --check`.
  6. Repeat all six exact post-resolution normal/default/all-feature/host-target
     trees and three inverse trees, mechanically confirm no dependency or
     feature drift from the audited post snapshots, then repeat release artifact
     sizes, generated TS SHA/absence checks, production unsafe/dependency-call/
     source audits, and `git status --short --branch`. Review the complete diff
     and confirm that only allowed Phase 1C files changed and `x` remains
     untracked and unstaged without inspecting it.

  Tool absence alone is not an advisory result: dated official RustSec and OSV
  queries remain mandatory for every identity in the exact A/B/C changed-graph
  audit union. Network or registry evidence unavailable at this gate is recorded
  and escalated to the Orchestrator and Security Reviewer before compiling or
  executing the changed dependency graph or any newly resolved dependency/
  project change. The required pre-edit release build measures only the
  unchanged signed baseline and is not prohibited by this post-resolution
  gate.
- **Required reviews and artifacts:**
  - `docs/evidence/phase-1c-dependencies.md` records baseline/final lock and
    generated-contract hashes; set A's exact newly-added full lock identities;
    all six exact baseline/post tree snapshots and mechanical diffs; exact sets
    B and C plus their derivation; and the final A/B/C union. For every union
    identity it records checksum/source/revision, cached-or-fetched local source,
    license/MSRV, duplicate/yank/advisory status, all unsafe and its feature
    reachability, build-script/proc-macro behavior, and enabled feature facts for
    every captured configuration. It also records the three inverse trees,
    allowed/forbidden API audit, dated RustSec/OSV results and explicit tool/
    network limitations, exact before/after sizes for the `rehow-core` rlib and
    `rehow-mcp`/`rehow-desktop` binaries, and every discrepancy. Unrelated
    downloads are explicitly excluded. Evidence contains no source, Guide body,
    scalar, or comment content.
  - `docs/evidence/phase-1c-parser-matrix.md` maps every failure kind and
    reachability class, limit, round-trip reason, ADR vector, combined
    precedence boundary, tail case, test seam, property seed/count, complexity
    counter, trait/API negative, and dependency call to a named passing test.
    Record exact test counts, commands, and environment.
  - Orchestrator/high inspects the actual implementation, dependency imports,
    manifest/lock diff, generated-file identity, artifacts, test controls,
    source/evidence, and full command output rather than accepting the Worker
    summary.
  - Independent Reviewer/high must pass correctness, API/trait minimality,
    deterministic precedence, fixture completeness, maintainability, and
    evidence accuracy. Security Reviewer/xhigh must pass hostile-input bounds,
    both parser surfaces, unsafe/dependency/advisory review, tail behavior,
    sanitized failures, and test-control absence. Both must report no unresolved
    blocker/high/medium finding before integration.
  - No UI, screenshot, design, accessibility, filesystem, Git, MCP, or manual
    Workspace artifact is required because this phase exposes no such surface.
- **Explicit non-goals:**
  - No Manifest, Category, Guide, Attachment, known/reserved-field, collection
    cardinality, cross-field, status, verification, compatibility, or migration
    aggregate validation; Phase 1D owns typed binding and known-field policy.
  - No deterministic YAML emitter, serialization, span edit, replacement,
    insertion/removal site, semantic effect set, body save, timestamp/status
    companion edit, or `UNSAFE_ROUND_TRIP` construction. Reconciled ranges are
    evidence only and confer no edit authority.
  - No Workspace path, file read/write, capability, revision/hash, atomicity,
    recovery, watch, import, Markdown-body parse/render, search, Git, Tauri,
    desktop UI, MCP, logging, network, clock, entropy, or process behavior.
  - No public Rust parser API, Serde/TS representation, generated-contract
    change, dependency re-export, retained parser object, public test hook,
    configuration knob, relaxed limit, or alternate/fallback parser.
  - No dependency other than the three exact runtime pins; specifically no
    regex, bigint, property/fuzz, snapshot, assertion, error, async, or unsafe
    helper crate and no package/tool/runtime installation.
- **Definition of done:**
  - Only allowed files differ from the signed packet base; lock movement is
    fully attributable to the exact three pins and all features match ADR 0013.
    Set A preserves every newly-added full lock identity; the six exact
    baseline/post tree diffs mechanically establish newly reachable set B and
    changed-feature set C for all configurations to be compiled. Every member
    of the exact A/B/C union has cached-or-fetched local-source evidence for
    checksum/source/revision, license, MSRV, feature-reachable unsafe,
    build-script/proc-macro behavior, enabled features, duplicates, yank, and
    advisories, while unrelated downloads are excluded. That review passed
    before compiling/executing the changed graph, and final repeated trees show
    no drift from the audited post-resolution graph.
  - Both exact entry points implement every envelope, semantic, presentation,
    guard/tail, reconciliation, comment, artifact, failure, precedence, and
    bounded-work rule without unsafe or forbidden dependency calls.
  - The crate-private API and trait surface match every ADR 0013 signature and
    absence requirement; normal rustdoc/external trybuild cannot see parsing or
    test controls; the Phase 1B public API remains unchanged.
  - Every source-reachable failure has a natural fixture and its
    source-reachable limits have natural exact/one-over evidence. Every
    differential/invariant failure, including semantic encoding/comparison
    limits, has the exact direct/fault/bridge evidence only; all ten read-only
    reasons and all precedence/tail/comment/property cases pass.
  - Accepted artifacts retain raw source, including body/comment bytes, exactly
    once; it is reachable only through prescribed crate-private accessors and
    checked ranges, and comment records copy no text. Artifacts expose complete
    semantics and are `Proven` only with full correspondence. Only otherwise
    accepted unguarded disagreement becomes `ReadOnly`; guarded/parser/
    invariant failure is sanitized `Err`, and no outcome fabricates a node/
    entry/comment relationship or mutation authority.
  - Portable count/allocation formulas and `TailDescriptor` size/alignment are
    asserted. Other artifact-record `size_of` values are recorded only as
    target-specific observations, never invented as portable constants.
  - Generated TypeScript is byte-identical and contains no forbidden Phase 1C
    or unknown-YAML declaration. Both evidence files truthfully match the
    implementation, lockfile, dependency sources, commands, and counts.
  - Focused and complete gates pass on unchanged installed Node 26.2.0, pnpm
    11.20.0, and Rust/Cargo 1.98.0. Reviewer/high and Security Reviewer/xhigh
    approve the actual diff with no blocker/high/medium finding.
- **Expected commit boundary:** One coherent Phase 1C implementation checkpoint
  contains the exact three dependency pins/lock delta, crate-private parsing
  implementation, the minimal test-only Phase 1B meter bridge, private parser
  tests/fixtures, six API negatives, and both evidence files. It contains no
  task-packet/progress update, unrelated cleanup, generated TypeScript change,
  or `x`. The Orchestrator stages only enumerated files, inspects the staged diff
  and whitespace, creates the signed implementation commit after all gates and
  reviews pass, verifies its signature, then updates `PROGRESS.md` in a separate
  signed ledger checkpoint naming the verified hash and exact next Phase 1D
  architecture gate.
- **Conditions requiring escalation:** Stop without improvising and report the
  exact evidence if a prerequisite/signature/base differs; another writer owns
  an allowed path; Phase 1B API/builder counters differ from ADR 0013; a listed
  dependency/API/SyntaxKind/event shape differs from the pinned sources; Cargo
  changes an existing lock entry; a baseline/post tree is missing or cannot be
  mechanically diffed; A/B/C membership is ambiguous; any union member is
  yanked/advised, has a license/MSRV/source/checksum/revision/feature-reachable-
  unsafe/build-script/proc-macro discrepancy, enables unexpected behavior,
  lacks its exact local source whether cached or fetched, or cannot be fully
  reviewed before the changed graph is compiled/executed because source/
  network/registry/advisory evidence is unavailable; a final tree drifts from
  the audited post snapshot; an
  exact parser call or required fixture is infeasible; complete coverage,
  linear work, allocation bounds, or sanitized failure cannot be proven; a
  natural witness contradicts ADR 0013's reachability classification; a parser
  panics or fails to terminate within the bounds; generated TS changes; a test
  needs a wider hook, lower production limit, new dependency, filesystem,
  public API, serialization, product-field decision, or mutation authority;
  required commands fail outside the packet; or review finds an unresolved
  correctness/security issue. Any change to signatures, visibility, ownership,
  traits, limits, precedence, scalar grammar, comment/reconciliation algorithm,
  malformed/read-only split, dependency pins/features/calls, or phase authority
  requires Architect/xhigh and Security Reviewer/xhigh review before work
  resumes.
