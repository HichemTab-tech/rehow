# Task Packet: Phase 1C parser correction and closure

- **Objective:** Correct the rejected partial Phase 1C implementation in place
  until the complete dirty diff implements accepted ADR 0013 literally and
  passes its production-backed parser, reconciliation, resource, precedence,
  API, dependency, and evidence gates. Preserve the already-reviewed
  dependency resolution and its completed precompile gate. The outcome is one
  accepted, crate-private generic parser checkpoint: exact source bytes are
  retained once, Guide bodies remain opaque, semantic and presentation traces
  are bounded and independently reconciled, hostile input cannot induce
  unbounded work or content disclosure, and no typed-product or mutation
  authority is added.
- **Required reasoning level:** medium. One Integrator using
  `gpt-5.6-sol`/medium owns all four sequential boundaries below. The current
  defects cross source classification, two-parser state, allocation lifetime,
  failure precedence, and correspondence, so a Worker/low must not redesign or
  independently join those data flows. After the Integrator freezes the
  production record/reducer/test-control shapes at the end of Boundary 3, the
  Orchestrator may delegate only mechanical, explicitly named fixture cases,
  matrix rows, or evidence transcription from passing commands to one
  Worker/low; the Integrator remains the sole owner of production code and
  reviews every such addition. No other model is permitted.
- **Prerequisites:**
  - Read `AGENTS.md`, `PROGRESS.md`, this packet, the signed original packet at
    commit `c98ab13`, accepted ADRs 0011--0013, and every specification named
    below. Inspect the complete current implementation, test, fixture,
    manifest/lock, and evidence diff rather than relying on its green tests.
  - Verify `df067b2`, `f39d7ab`, and `c98ab13` with `git verify-commit`.
    `HEAD` at packet creation is the signed progress ledger
    `b083ceb4125cacc7dffd08a44cfbe8e4cc56c39d`; implementation changes are an
    intentional uncommitted continuation after it.
  - Before Integrator dispatch, this closure packet must itself have a later
    reviewed signed packet checkpoint, followed by a separate signed
    `PROGRESS.md` recovery-ledger checkpoint. `PROGRESS.md` records the exact
    closure-packet hash and names this packet/Integrator-medium closure as the
    next action. A commit cannot contain its own hash; instead, the Integrator
    verifies the packet signature, verifies current `HEAD`'s signature,
    confirms `HEAD^` is the packet checkpoint, confirms the ledger commit
    changes only `PROGRESS.md`, and confirms both `PROGRESS.md` and this packet
    match signed `HEAD` in the index and worktree.
    That verified post-ledger `HEAD`, not `b083ceb`, is the Git comparison base
    for implementation-diff scope and final staging; the hashes below
    independently freeze the rejected implementation content that remains
    dirty across the documentation-only checkpoints.
  - The original packet's pre-edit release measurement, six baseline trees,
    single `cargo fetch`, A/B/C supply-chain audit, Security Reviewer approval,
    and first changed-graph command
    `cargo check -p rehow-core --locked` have already completed and passed.
    `docs/evidence/phase-1c-dependencies.md` is the durable evidence. Do not
    repeat `cargo fetch`, re-resolve, edit a pin, or describe the gate as still
    pending. Re-run locked non-resolving checks only.
  - Freeze the current dirty base before writing. These SHA-256 values must
    match exactly:

    ```text
    Cargo.lock                                      a0b7fd09e28f8676543d6d8fffd8664ca60b0979f51776a74a5f5e27311290d5
    crates/rehow-core/Cargo.toml                    93d1895b679c25c7d510536e7c43eb54ff17b79dd08d029afe15e3b024b59749
    crates/rehow-core/src/lib.rs                    7239c5f3c2cea425473f2b93bd050d5d40f31b5d276ac71f270d88968891cea4
    crates/rehow-core/src/domain.rs                 2b6e256d034cdf1d52b0aa33e54863c1561f31498c3f0fa1c14198104a05bb2d
    crates/rehow-core/src/domain/yaml_value.rs      a3fd1766b0d804cc1dc009ee4b55361bed259f916d9fd59cdeb4570b51d549d7
    crates/rehow-core/src/parsing/mod.rs            a947cca074853a52b79e7b61936bf0d0f6c7b50eb5c33eb7325f4fa759d90359
    crates/rehow-core/src/parsing/source.rs         07156eb18d8e0d0118372c30d942bdaf8bf627eebcd05875c3300a5a0bb978de
    crates/rehow-core/src/parsing/presentation.rs   e529d951da8c6193981beac19676d622a5602f5a126b9483aeb1ef5811122980
    crates/rehow-core/src/parsing/semantic.rs       5fd23743e29667a01cc6f3bda388dce10537cd64205ee676b22be8c2740c1cfd
    crates/rehow-core/src/parsing/scalar.rs         4bbcca9599c2d1a08e51bcaf9a28678d74896a72fa4ef803e6b7ab7c490d11cb
    crates/rehow-core/src/parsing/reconcile.rs      90ebe47f1c20184e16148d0aaf2f25d67e17ef89d7ae5a486006cb38e2d31c1f
    crates/rehow-core/tests/public_api.rs           5805157fa508a121479a870574a99cd778ea2a24267dce9efe025614cad7cbaa
    docs/evidence/phase-1c-dependencies.md          c82708ac73fe6c2463a59bf4a35fd65094f7e93bdb16f3b4f6981fafff562a34
    docs/evidence/phase-1c-parser-matrix.md         0b63567853833756a7f4857dfcf943de3d7258c1be6cb93e49e7b02c660f90e6
    ```

    The sorted content-hash digest of every current file under
    `crates/rehow-core/src/parsing/tests/` and
    `crates/rehow-core/tests/ui/` is
    `4c94bbb7db04b6d5c1f166ed5a3c8f7031e73c91a517b35e2dbb9d57ce5ed175`.
    A mismatch means another writer changed the rejected base: stop and ask the
    Orchestrator to reconcile ownership before editing.
  - The unrelated untracked root executable `x` is never read, executed,
    hashed, modified, staged, deleted, or included in evidence. Use explicit
    pathspecs when inspecting or staging. Use the installed Node 26.2.0, pnpm
    11.20.0, and Rust/Cargo 1.98.0 unchanged. Do not invoke a tool/runtime
    installer or a dependency add/update/resolution command. The final gate's
    exact existing-tool `pnpm install --frozen-lockfile` is permitted only to
    materialize the already-locked dependency tree; it must not change
    `pnpm-lock.yaml` or either installed runtime.
- **Allowed files or subsystem:** The Integrator has exclusive sequential write
  ownership only of:
  - `crates/rehow-core/src/parsing/mod.rs`, `source.rs`, `presentation.rs`,
    `semantic.rs`, `scalar.rs`, and `reconcile.rs`;
  - files below `crates/rehow-core/src/parsing/tests/`, including byte fixtures
    only below its existing `fixtures/accepted/`, `fixtures/malformed/`, and
    `fixtures/read-only/` roots;
  - `crates/rehow-core/src/domain.rs` and
    `crates/rehow-core/src/domain/yaml_value.rs`, only if the existing exact
    ADR 0013 `#[cfg(test)]` meter bridge or its tests need correction;
  - `crates/rehow-core/src/lib.rs`, only to preserve the exact single
    module-level `dead_code` allowance and reason frozen by ADR 0013;
  - `crates/rehow-core/tests/public_api.rs` and the six existing Phase 1C
    source/`.stderr` pairs below `crates/rehow-core/tests/ui/`;
  - `docs/evidence/phase-1c-dependencies.md` and
    `docs/evidence/phase-1c-parser-matrix.md`.

  `Cargo.lock` and `crates/rehow-core/Cargo.toml` are part of the final dirty
  implementation checkpoint but are frozen read-only at the hashes above. The
  three pins and resolved graph are already accepted; no owner may edit them.
  `packages/contracts/src/generated.ts` is read-only and must retain SHA-256
  `156c56374875090606dde45f6141b8f2a22a8eeeb3d1359423edf696893d3511`.
  Do not edit any existing task packet, ADR, specification, `PROGRESS.md`,
  contract generator, adapter, UI/Tauri/MCP source, agent config, root script,
  other core module, or Phase 1B fixture. Only one owner writes these paths at
  a time; no parallel production edits. Do not stage or commit.
- **Relevant specifications and ADRs:**
  - `AGENTS.md`: hostile-content, Rust-core, no-execution, source-retention,
    testing, model, orchestration, and commit invariants.
  - `docs/task-packets/phase-1c-parser-reconciliation.md` at `c98ab13`: exact
    original scope, dependency gate, public-absence gate, full commands, and
    final artifact obligations; this closure packet tightens correction order
    without relaxing any requirement.
  - `docs/adr/0013-phase-1c-parser-and-reconciliation-contract.md` at
    `f39d7ab`: normative exact APIs, traits, CST vocabulary/contexts,
    descriptors, pending buckets, reducers, meters, precedence, artifact,
    ranges, comments, fault seams, stable test names, and evidence.
  - `docs/adr/0011-bounded-yaml-semantics-and-lossless-span-edits.md`: YAML
    subset, source/semantic limits, numeric `S`/`C`/`R` charging, generic
    reconciliation, and no mutation authority.
  - `docs/adr/0012-phase-1b-protocol-construction-and-dependencies.md`: actual
    `UnknownYamlDocumentBuilder`, exact builder precedence/meters/errors,
    private tree construction, and trait surface.
  - `docs/architecture.md` (`rehow-core`, dependency policy, disclosure/failure
    behavior), `docs/content-model.md` (frontmatter, unknown YAML, parser
    limits), `docs/implementation-plan.md` (Phase 1 milestone 3 and 1C/1D
    split), `docs/testing-strategy.md` (YAML/adversarial/gates),
    `docs/threat-model.md` (hostile YAML, bounds, dependency and disclosure
    review), and `docs/error-contract.md` (future diagnostic mapping only).
  - When abbreviated wording here differs from accepted ADR 0013, ADR 0013 is
    authoritative. Apparent contradiction is escalation, never permission to
    select a simpler behavior.
- **Exact required behavior:** Implement these four boundaries in order. The
  Integrator records the boundary diff and runs its exact focused commands
  before beginning the next; a green command does not waive source inspection.

  **Boundary 1 — source facts and exhaustive presentation normalization**

  1. Keep the public-to-siblings entry signatures and artifact/accessor/trait
     surface byte-for-byte equivalent to ADR 0013. Refactor only private data
     flow so `parse_guide_source` validates complete-file UTF-8 but passes a
     borrowed, bounded byte-zero-through-separator comment region alongside
     the YAML slice; `parse_yaml_document` passes its bounded whole source.
     Never copy or scan the Guide body for YAML policy, comments, lines, tags,
     punctuation, or Markdown. After complete-file UTF-8 validity, body bytes
     containing raw U+FEFF, bare CR, mixed endings, `&`, `*`, `%`, `!`, `#`, or
     delimiter-like lines remain opaque and cannot affect the YAML result.
  2. Replace early-return scanner policy with fixed-size fact collection and
     the exact global ordering: source byte cap; complete UTF-8 with no
     position; raw BOM then bare CR then mixed YAML/envelope endings; Guide
     opening, closing/frontmatter cap, then separator. Preserve exact limit
     scopes, accepted maximum, and first refused/checked-lower-bound facts.
     For a Guide with no proven separator/body boundary, scan only through the
     frontmatter/whole-file bound needed to classify it; never infer that the
     remainder is body. Keep `InvalidUtf8.position() == None` for every byte
     location.
  3. Delete the handwritten text-scanning `source_policy_precheck` or any
     equivalent pseudo-lexer. Source policy comes only from the byte scanner
     and the single exhaustive CST token/context walk. Valid scalar contents
     are never rejected because they contain punctuation; in particular a
     literal or folded block scalar containing `&`, `*`, `%`, `!`, `---`,
     `...`, `#`, quotes, braces, or brackets parses according to its CST.
  4. Implement the exact normalized context grammar in ADR 0013: `ROOT` has
     zero or more `DOCUMENT`; each `DOCUMENT` has exactly one semantic payload;
     mappings/sequences contain only their respective entry wrappers;
     `MAPPING_ENTRY` has exactly one `KEY` then one `VALUE`; `KEY`, `VALUE`, and
     `SEQUENCE_ENTRY` have exactly one payload; `TAGGED_NODE` has exactly one
     direct `TAG` and one scalar/sequence/mapping payload with no nested tag;
     `SCALAR` has no semantic descendant; `ALIAS` is denied and `ERROR` is
     shape failure. Enforce punctuation/trivia only in its exact block/flow,
     wrapper, and scalar context. Both node and token arms explicitly match all
     50 `SyntaxKind` variants listed in ADR 0013 with no wildcard; do not
     reduce this to a kind allowlist divorced from parent/child counts/order.
  5. Preserve exact selected positioned-error ordering, complete ordered leaf
     coverage, token/source equality, optional single final zero-width EOF,
     and exact source no-op check. The sole other zero-width leaf is the one
     zero-width `SCALAR` containing one zero-width empty `NULL` in a `KEY`,
     `VALUE`, or `SEQUENCE_ENTRY`. Its matching Saphyr plain `~` is an
     `ImplicitEmpty` and can be `Proven`; valid `a:\n` must not become
     read-only. All other zero-width candidates are malformed shape.
  6. Build `SourceGuard` exactly once from checked source bytes. Retain complete
     range/decoded/raw records only for the first 20,000 presentation semantic
     nodes. Retain one 16-byte/alignment-4, range/text/ID-free
     `TailDescriptor` for every candidate through the checked
     `2 * yaml_len + 1` cap, including all candidates after node 20,000. Check
     every multiplication/addition/capacity before allocation. Do not first
     allocate a rich record for the tail and discard it later. Enforce the ADR
     aggregate presentation decoded/raw copy bounds while constructing the
     prefix, and retain no dependency object or line table in the result.

  **Boundary 2 — production bounded Saphyr reducer, scalar meters, and builder**

  7. Replace the current early-error drain with one production reducer that
     calls only `Parser::new_from_str`, `keep_tags(false)`, and bounded
     `next_event`. It owns the exact five pending buckets: scan; stream/
     multiple-document; parser tag/merge differential; event-dependent source
     restriction; semantic structural/limit. A first semantic error clears
     all rich success events and decoded copies, but does not synthesize a
     `SemanticStreamOrder`, stop structural reduction, or skip later guard/tail
     validation. Drain through final `None` unless Saphyr returns `Err`, the
     event-call invariant trips, or ADR 0013 requires dependency calls to stop.
     Select the result only after the drain by the global 14-stage precedence.
  8. The same production reducer, not a test imitation, constructs each event
     ordinal/parent/role/kind tuple with a checked lightweight stack, computes
     `EventGuard`, and compares guards only while the source/event tuple prefix
     remains proven. A present tag is inspected by the exact fixed-length core
     URI rule; a tagless plain mapping key reads decoded text to distinguish
     exact `<<`; other tagless nodes are `None`. At first tuple disagreement,
     break correspondence permanently, visit each remaining source descriptor
     and event exactly once without ordinal-zipping or resynchronization, and
     report any proved later non-none source/event guard. Implement
     source-none/event-merge, source-merge/event-none, shifted Saphyr-only
     merge, and explicit-tag asymmetry before and after node 20,000.
  9. Implement the parser-owned monotonic borrowed-text counter and
     `Uninspectable` state exactly. Compute
     `ceil(3 * yaml_len / 2) + D` with checked arithmetic; charge `text.len()`
     once immediately before permitted inspection. At equality continue. On
     conversion/addition overflow or one-over, record
     `TailBorrowedTextInvariant`, disable current and all later text reads,
     copies, and scans permanently, yet continue O(1) structural reduction and
     exact present-tag checks. A tagless plain mapping key then becomes
     `Uninspectable`, never guessed. Charge and assert tail work no greater
     than `2 * D + event_call_limit + borrowed_text_limit`.
  10. Retain at most 20,000 node events, 20,000 collection-end events, and four
      stream/document records. The post-failure reducer retains only bounded
      descriptor/stack/pending facts. Enforce temporary presentation plus
      semantic decoded copies at `3 * yaml_len + 2 * node_count` and all
      copied decoded/raw text at `4 * yaml_len + 2 * node_count`; check vector
      payload sizes and capacities before allocation. No marker, span,
      dependency error/message, parser object, or rejected content survives.
  11. Make scalar classification and conversion obey the literal ADR 0013
      grammar and exact same-event order. Compute `S`, `R`, and the closed-form
      `R + 4*S` precharge without allocating the canonical integer/float
      coefficient/exponent. Reject precharge overflow/limit before radix
      vectors, `format!`, concatenation, trim-to-owned, or any result-sized
      allocation. After bounded conversion, compute `C`, charge `4*C`, enforce
      the 65,536 per-scalar and 81,920 aggregate digit limits, then retain the
      canonical event. No regex, bigint, `f64`, full machine-width integer,
      hidden quadratic string edit, or uncharged allocation/work is allowed.
  12. Mirror only depth, nodes, completed mapping/sequence entries, and numeric
      digits, in the exact accepted event order, and keep the active builder
      charge scope alongside each real `push_event`. Fold one retained event
      to one fresh `UnknownYamlDocumentBuilder` only after the drain and guard
      gates pass. Map `ArithmeticOverflow` to the meter actually active at that
      call (`Nodes`, `NumericDigits`, `SemanticEncoding`, or
      `SemanticComparison`), never unconditionally to semantic encoding; map
      every other builder kind exactly as ADR 0013. A mirror/builder mismatch,
      poison, impossible accounting, event-call fault, or tail invariant is
      `SemanticBuilderInvariant`; finish only one closed implicit mapping
      document with string top-level keys. No partial trace/tree survives.

  **Boundary 3 — monotonic reconciliation, range proof, and linear comments**

  13. Replace permissive index/lookup logic with one preorder zip over the two
      Rehow-owned traces and compare all eight ADR dimensions in order:
      parent/role; node kind with only the exact implicit-empty pair; closed
      arity and child roles; validated source tag; scalar style; yaml-edit
      decoded versus Saphyr pre-resolution text; yaml-edit raw value versus
      checked payload source; candidate range validity/containment. Never
      search for a later match. At first structural mismatch emit exactly one
      issue, permanently mark the semantic suffix `Unreconciled`, and populate
      every suffix node only from Saphyr semantic facts, never from the same or
      a shifted presentation ordinal. An unreconciled scalar or implicit-empty
      node retains only its Saphyr decoded text and Saphyr scalar style in
      `decoded_scalar`/`scalar_style`; its `collection_style` is `None`. An
      unreconciled sequence or mapping has `decoded_scalar == None`,
      `scalar_style == None`, and `collection_style == None` even though
      Saphyr supplied collection syntax: block/flow style is presentation-only
      and is not inferred after correspondence breaks. No tag, range, entry,
      style, or other suffix field is copied from a presentation ordinal after
      the break.
  14. Build entries during the same monotonic proven-prefix traversal or from
      bounded direct child-index tables derived by counting/prefix passes.
      Never resolve one entry by scanning all semantic nodes. Retain only the
      longest contiguous normalized presentation-entry prefix whose parent and
      referenced key/value/item nodes were all proven before the break. Assign
      contiguous `EntryId`s from zero; do not create IDs from semantic suffix
      roles or skip a failed entry and resume later. Node `entries` lists name
      only that exact retained prefix.
  15. Prove ranges before constructing `NodePresentation::Concrete` or an
      entry: UTF-8 boundaries and YAML bounds; tag and payload within whole;
      child whole range within every reconciled ancestor; siblings disjoint
      except ancestor containment; entry wrapper contains its direct item or
      key/value payloads; sibling entries disjoint; exact one-time checked file
      rebase. Range/rebase/overlap failures are ordered read-only issues, not
      panics or malformed errors. Implicit empty has only a checked site.
  16. Implement ADR 0013's comment algorithm literally. Scan the bounded
      region once into checked `Box<[u32]>` line starts and parallel
      `Box<[u8]>` blank flags, at most `R + 1` each; do not scan a Guide body.
      During one iterative CST enter/leave traversal, record each comment's
      deepest candidate node, entry, and collection. After reconciliation use
      two counting/prefix-sum passes to bucket proven entries and comments by
      parent, then one monotonic two-cursor merge for maximal comment-only runs.
      Do not slice from byte zero for each comment, repeatedly call `parent()`,
      sort by content, scan all entries for a comment, or scan all nodes for an
      entry. Apply exact inline/nested/block leading/trailing/inter-entry/
      document/flow/ambiguous ownership. A crossed/missing range, two deepest
      owners, dropped entry, unreconciled node, or scalar overlap without CST
      descent must naturally produce `Unresolved`/`Ambiguous` and the matching
      read-only issue.
  17. Use a production-owned test work counter seam to charge source bytes,
      syntax elements, reconciliation nodes, entry cursors, and comment
      cursors once; assert checked
      `work <= 12*S + 8*T + 12*N + 8*E + 8*C + 64`. Drop line tables,
      ancestor/bucket scratch, both parser handles, all tail traces, and other
      temporary storage before return. Confirm the final allocation inventory
      row-by-row against ADR 0013 and retain only the one source box, semantic
      tree/caches, at-most-20k reconciled records, at-most-19,999 child and
      entry IDs, source-range comments, bounded decoded/canonical strings, and
      at-most-32 issues.

  **Boundary 4 — production-backed controls and acceptance proof**

  18. Remove `Presentation::forced_issue`, direct issue injection, the toy
      test-only `TailReducer`, and any test that merely reimplements expected
      ranking or mutates a final report. Every private fault control must alter
      a copied Rehow-owned temporary production record, pending bucket,
      reducer state, event return, range, or meter after dependency calls, then
      run the same production reconciliation/selection path. The unnameable
      `#[cfg(test)]` control covers every ADR 0013 bullet and cannot construct
      semantic values/artifacts, change a production constant, inject YAML,
      or survive normal/rustdoc/external builds. Retain the exact Phase 1B
      builder-meter bridge only; test Active and Poisoned states with one real
      nonzero charge for all four meters.
  19. Complete the checked-in byte fixture corpus and named matrix. Every one
      of 32 source-reachable failure kinds has a deterministic natural parse
      case; every source-reachable limit has natural exact and one-over input
      with exact scope/limit/observed facts; three parser-differential and six
      invariant/fault-only kinds use production-backed direct/fault evidence
      and are never mislabeled natural. Cover all 41 kinds exactly once by
      reachability, all ten read-only reasons, both entry APIs, LF/CRLF/BOM/
      final-newline combinations, all scalar/collection styles and built-in
      tags, complex keys, comments, implicit empties, nesting, duplicate
      semantics, all ADR 0011 scalar/numeric vectors, every adjacent global
      precedence pair from stages 5 through 14, the exact source/envelope
      precedence combinations for stages 1 through 4, and ADR 0013's
      cross-stage cases.
  20. Add all 14 stable guarded-tail tests verbatim, backed by the production
      reducer and with their exact precedence assertions:

      ```text
      guard_source_none_event_merge
      guard_source_merge_event_none
      guard_shifted_saphyr_only_merge
      guard_shifted_saphyr_only_merge_after_node_20000
      tail_borrowed_text_repeated_scalar_maxima_at_limit
      tail_borrowed_text_one_over_is_invariant
      tail_borrowed_text_counter_overflow_is_invariant
      tail_borrowed_text_limit_arithmetic_overflow_is_invariant
      tail_borrowed_text_disable_stops_all_later_scans
      tail_borrowed_text_invariant_then_scan_error
      tail_borrowed_text_invariant_then_stream_error
      tail_borrowed_text_invariant_then_exact_tag_disagreement
      tail_borrowed_text_invariant_then_shifted_source_guard
      tail_borrowed_text_invariant_then_uninspectable_merge
      ```

      The first three also run beyond node 20,000. Add explicit rich-prefix/
      descriptor-only-tail allocation assertions, exact equality/one-over/
      checked overflow cases, and proof that the current and later borrowed
      text are never read after disablement.
  21. Strengthen the fixed-seed xorshift64 harness to at least 4,096 arbitrary
      byte cases and 4,096 mutations across the accepted fixture set. It must
      assert bounded completion/no panic and, for every `Ok`, traverse and
      prove root/ID/parent/role/child/entry contiguity, eight-dimension
      correspondence prefix, source/range/rebase/ancestor/sibling/entry
      invariants, comment IDs/owners/ranges, semantic traversal termination,
      report order/count/truncation, and retained allocation/count formulas.
      A call whose result is ignored is not evidence.
  22. Make `dependency_surface` an exact compile/source audit, not a few string
      exclusions: exercise every allowed yaml-edit/rowan/Saphyr call in one
      private adapter; enumerate production imports and qualified calls;
      require both exhaustive `SyntaxKind` matches and the exact dead-code
      reason; reject the complete forbidden catalogs from ADR 0013, retained
      dependency types, `unsafe`, recursive Rehow traversal, and pseudo-lexer
      helpers. Generate rustdoc normally and assert no parsing/artifact/failure/
      coordinate/comment/round-trip/test-control symbol is externally visible.
      Preserve all six trybuild negatives and the Phase 1B positive surface.
  23. Update both evidence files only from actual final source and passing
      output. Dependency evidence preserves the completed precompile history,
      corrects no fact speculatively, adds final six/inverse-tree no-drift,
      exact import/call/rustdoc audits, final hashes/sizes, and discrepancies.
      Parser matrix names the concrete test/fixture for every kind, limit,
      stage pair, builder mapping/scope, read-only reason, tail case, comment
      placement/ambiguity, allocation/work bound, property invariant, and API
      absence; record exact counts/seeds/commands. Evidence contains no Guide,
      scalar, command, comment, rejected input, path credential, dependency
      error, or other content disclosure.
- **Important states and edge cases:**
  - Empty whole YAML; empty/multiple parser documents; scalar/sequence roots;
    incomplete mappings; exact zero-width omitted key/value/item versus source
    `~`; complete Guide with empty or blank-leading body; frontmatter exactly
    262,144 through separator and one over; Guide exactly 2,097,152 and one
    over; close at EOF/no separator; indented delimiter; BOM at byte zero,
    YAML/envelope interior, and opaque body; invalid UTF-8 before/after LF and
    CRLF and incomplete final sequence; mixed/bare endings only in YAML versus
    body.
  - Valid block/folded/quoted/plain scalar punctuation, every exact allowed and
    denied tag form, anchors/aliases, merge spellings, escaped noncharacters,
    decoded expansion exact/one-over, raw/canonical/numeric/digit exact/
    one-over, signed zeros, arbitrary exponents, large octal/hex precharge,
    semantic duplicate keys and 256th/257th entry precedence.
  - Exactly 16 depth, 20,000 semantic nodes, 256 mapping entries, 2,048
    sequence entries, every one-over, source descriptors beyond node 20,000,
    later scan/stream/tag/merge conditions after an earlier semantic failure,
    borrowed-text equality/one-over/overflow/Uninspectable, and all guarded
    shifts without resynchronization.
  - First mismatch at root/key/value/nested child/entry wrapper; missing/extra
    traces; tag/payload/whole/ancestor/sibling/entry range failures; contiguous
    entry cutoff with no suffix ID; semantic suffix styling; exact 32/33 issue
    truncation and checked total overflow.
  - Comments inline, scalar-nested, leading, trailing, inter-entry, flow,
    document-only, ambiguous/crossed, next to dropped entries and unreconciled
    nodes; dense-newline `R == 262,144`; dense comments, deepest allowed
    nesting, mixed flow/block, and comments at every boundary under the linear
    work counter.
- **Security constraints:**
  - Treat bytes, dependency CST/events, decoded text, tags, comments, and
    filenames as hostile. No arbitrary filesystem/path, network, process, Git,
    Tauri, MCP, rendering, logging, command execution, serialization, or
    mutation authority enters Phase 1C. Tests use in-memory boxes and checked-in
    fixtures only.
  - Production parsing contains no `unsafe`; uses only the audited dependency
    pins/features/calls; never trusts Saphyr markers/spans or dependency
    diagnostics; retains no dependency object; performs no recursion, regex,
    bigint, unmetered sort/search, or parser fallback. Do not change the frozen
    dependency graph or install a tool.
  - Check every integer conversion/add/multiply and allocation capacity before
    allocation. Enforce byte, record, descriptor, stack, decoded/raw,
    canonical-digit, builder, comment, issue, event-call, borrowed-text, tail,
    and linear-work bounds in production. Equality at a fixed product limit is
    accepted; the first refused value is reported exactly.
  - Returned failures have fixed non-content `Display`/`Debug`, no source
    chain, and only kind/position/limit facts permitted by ADR 0013. Drop all
    rejected source/text/events before `Err`; never write hostile content,
    dependency messages, markers, body/comment text, absolute paths, secrets,
    or credentials to evidence or logs.
  - Guarded disagreement, parser failure, source/semantic restriction,
    resource failure, or invariant poison is always sanitized `Err`, never
    read-only. Only a fully accepted, unguarded presentation/correspondence
    disagreement may return immutable `ReadOnly`; `Proven` requires every
    semantic, correspondence, range, comment, and no-op proof. No range or
    record grants edit permission.
  - Fault controls are private `cfg(test)` only, inject after dependency calls
    into temporary Rehow-owned state, and disappear from normal code, rustdoc,
    external crates, TypeScript, desktop, and MCP builds.
- **Tests to add or update:**
  - Keep the private modules `source`, `presentation`, `semantic`, `scalar`,
    `reconcile`, `comments`, `precedence`, `limits`, `faults`, `properties`,
    and `dependency_surface`, but replace shallow cases with named tests of the
    actual production paths/reducers. Add narrowly named fixture files beneath
    the three approved roots; do not put arbitrary generated test output in the
    repository.
  - `source`: exhaustive envelope/fact precedence, UTF-8 no-position vectors,
    exact/one-over file/frontmatter sizes, and opaque Guide-body sentinels.
    `presentation`: all 12 node and 50 total `SyntaxKind` contexts, error-range
    ordering, leaf faults, valid block-scalar punctuation, tags/merge source
    guards, implicit-empty shape, and 20k-prefix/tail allocation.
  - `semantic`/`scalar`/`limits`/`precedence`: real bounded drain and five
    buckets; exact stream state; every event/guard/tail transition; literal
    grammar/normalization; preallocation numeric charges; mirror/builder active
    scopes; 41-kind reachability; exact facts; all adjacent/cross-stage
    precedence. Use the ADR's natural decoded/canonical witnesses and its
    direct-only semantic encoding/comparison witnesses.
  - `reconcile`/`comments`: all eight dimensions in order, first-break suffix,
    contiguous IDs/entries, no shifted styling, full range proof, all comment
    placements/natural ambiguity, 32/33 report behavior, and maximum linear
    counter cases. No forced final issue/report injection.
  - `faults`: the 14 stable names above plus every copied-record, positioned
    range, event, return, bucket, mapper, mirror, range, comment, and arithmetic
    fault enumerated by ADR 0013. All assertions invoke the production reducer
    or parse continuation after the injected temporary state.
  - `properties`: both 4,096-case deterministic populations and full accepted
    artifact invariant validation. Record fixed seeds/counts.
  - `dependency_surface`: exact allowed/forbidden call/import catalog,
    exhaustive-kind compile guard, no production unsafe/pseudo lexer/recursion,
    exact lint reason, and rustdoc absence. `public_api` continues to run the
    six Phase 1C negatives with reviewed `.stderr` and unchanged Phase 1B pass.
- **Commands to run:** Run all commands from the repository root with the
  installed tools. No command may add/update/resolve dependencies or inspect
  `x`. Do not invoke a tool/runtime installer. The one explicitly listed
  `pnpm install --frozen-lockfile` is permitted frozen dependency
  materialization and must leave `pnpm-lock.yaml` byte-identical. Record exact
  exit codes and final test counts in evidence.

  1. Base gate before Boundary 1:

     ```text
     node --version
     pnpm --version
     rustc --version
     cargo --version
     git verify-commit df067b2
     git verify-commit f39d7ab
     git verify-commit c98ab13
     git verify-commit <reviewed-closure-packet-checkpoint-from-PROGRESS>
     git verify-commit HEAD
     test "$(git rev-parse HEAD^)" = "<reviewed-closure-packet-checkpoint-from-PROGRESS>"
     test "$(git diff-tree --no-commit-id --name-only -r HEAD)" = "PROGRESS.md"
     git diff --cached --quiet HEAD -- PROGRESS.md docs/task-packets/phase-1c-parser-closure.md
     git diff --quiet -- PROGRESS.md docs/task-packets/phase-1c-parser-closure.md
     sha256sum Cargo.lock crates/rehow-core/Cargo.toml crates/rehow-core/src/lib.rs crates/rehow-core/src/domain.rs crates/rehow-core/src/domain/yaml_value.rs crates/rehow-core/src/parsing/mod.rs crates/rehow-core/src/parsing/source.rs crates/rehow-core/src/parsing/presentation.rs crates/rehow-core/src/parsing/semantic.rs crates/rehow-core/src/parsing/scalar.rs crates/rehow-core/src/parsing/reconcile.rs crates/rehow-core/tests/public_api.rs docs/evidence/phase-1c-dependencies.md docs/evidence/phase-1c-parser-matrix.md
     find crates/rehow-core/src/parsing/tests crates/rehow-core/tests/ui -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum
     cargo check -p rehow-core --locked
     git diff --check -- . ':(exclude)x'
     python3 scripts/check_bootstrap.py
     ```

  2. After Boundary 1:

     ```text
     cargo test -p rehow-core --lib parsing::tests::source --locked
     cargo test -p rehow-core --lib parsing::tests::presentation --locked
     cargo test -p rehow-core --lib parsing::tests::limits --locked
     cargo fmt --all --check
     cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings
     git diff --check -- . ':(exclude)x'
     ```

  3. After Boundary 2:

     ```text
     cargo test -p rehow-core --lib parsing::tests::semantic --locked
     cargo test -p rehow-core --lib parsing::tests::scalar --locked
     cargo test -p rehow-core --lib parsing::tests::precedence --locked
     cargo test -p rehow-core --lib parsing::tests::limits --locked
     cargo test -p rehow-core --lib parsing::tests::faults --locked
     cargo fmt --all --check
     cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings
     git diff --check -- . ':(exclude)x'
     ```

  4. After Boundary 3:

     ```text
     cargo test -p rehow-core --lib parsing::tests::reconcile --locked
     cargo test -p rehow-core --lib parsing::tests::comments --locked
     cargo test -p rehow-core --lib parsing::tests::properties --locked
     cargo test -p rehow-core --lib parsing --locked
     cargo fmt --all --check
     cargo clippy -p rehow-core --all-targets --all-features --locked -- -D warnings
     git diff --check -- . ':(exclude)x'
     ```

  5. Boundary 4 API/contracts/docs and full acceptance gate:

     ```text
     cargo test -p rehow-core --lib parsing --locked
     cargo test -p rehow-core --test public_api --locked
     cargo doc -p rehow-core --no-deps --locked
     pnpm contracts:generate
     sha256sum packages/contracts/src/generated.ts
     pnpm contracts:check
     pnpm install --frozen-lockfile
     pnpm check
     pnpm test
     pnpm test:integration
     pnpm build
     cargo fmt --all --check
     cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
     cargo test --workspace --all-features --locked
     cargo test -p rehow-core --test public_api --locked
     cargo check -p rehow-core --lib --locked
     cargo build --release --workspace --locked
     pnpm tauri:build
     python3 scripts/check_bootstrap.py
     git diff --check -- . ':(exclude)x'
     ```

     Run ADR 0013's explicit forbidden TypeScript-name search and inspect the
     generated rustdoc for absence of every parser/artifact/failure/coordinate/
     comment/round-trip/test-control name. Repeat the original packet's six
     exact locked post-resolution Cargo trees and three inverse trees, compare
     them to the dependency evidence's audited graph, and record no drift.
     Repeat the exact release rlib/MCP/desktop sizes, production import/call/
     unsafe/source audit, and generated TypeScript SHA/diff. Finish with
     `git status --short --branch -- . ':(exclude)x'` and an explicit complete
     diff review against allowed paths. Do not run `cargo fetch`, unlocked
     Cargo commands, a dependency updater/resolver, or a tool/runtime
     installer. The required frozen pnpm materialization is the sole command
     named `install` here and must not alter `pnpm-lock.yaml`.
- **Required reviews and artifacts:**
  - At each boundary the Integrator supplies the Orchestrator a concise actual
    diff inventory, focused command output/counts, and a requirement-to-test
    map for that boundary. The Orchestrator inspects the source and diff before
    allowing the next boundary; it never accepts a green summary alone.
  - Final `docs/evidence/phase-1c-dependencies.md` retains truthful precompile
    gate history and adds final no-drift trees, exact dependency calls/imports,
    forbidden-call/unsafe/rustdoc results, lock/generated hashes, artifact
    sizes, and discrepancies. Final
    `docs/evidence/phase-1c-parser-matrix.md` exhaustively maps every accepted
    ADR 0013 obligation to concrete passing production-backed evidence.
  - Reviewer/high independently reviews correctness, exact API/trait surface,
    CST contexts, precedence/facts, scalar accounting, reconciliation/ranges,
    fixture/matrix completeness, maintainability, and evidence accuracy.
    Security Reviewer/xhigh independently reviews hostile-input termination,
    allocation/work bounds, pending/tail/Uninspectable behavior, tag/merge
    fail-closed semantics, sanitized failures, dependency/call/unsafe/no-drift
    evidence, Guide-body opacity, and test-control absence. Both must report no
    blocker/high/medium finding on the final actual diff.
  - If either review changes production behavior or security evidence, rerun
    the affected boundary commands and the complete final gate, then obtain a
    fresh review of the correction delta. No UI/design/accessibility screenshot
    gate applies because no UI surface changes.
- **Explicit non-goals:**
  - No ADR/specification/packet revision and no relaxation of an accepted
    limit, algorithm, fixture class, precedence, allocation formula, trait,
    error, or review criterion to preserve current code or make tests easier.
  - No Manifest/Category/Guide/Attachment field binding, reserved-field policy,
    typed aggregate, YAML emitter, span edit, insertion/removal site, semantic
    effect set, protocol error construction, or mutation authority; those begin
    in Phase 1D or later.
  - No filesystem/Workspace read or write, capability, revision, atomicity,
    recovery, import, Markdown-body parse/render, search, Git, Tauri command,
    desktop UI, MCP, log, network, clock, entropy, shell, or process feature.
  - No public parser API, Serde/TypeScript parser contract, dependency re-export,
    retained dependency handle, public/configurable fault hook, configurable
    production limit, alternate/fallback parser, or generated-contract change.
  - No new or changed dependency, feature, manifest/lock resolution, tool,
    runtime, fuzz/property/snapshot/regex/bigint/error/assertion helper crate,
    unrelated cleanup, staging, commit, push, or publication.
- **Definition of done:**
  - The frozen dependency pins/lock remain byte-identical, final Cargo trees
    match the audited post-resolution graph, generated TypeScript remains at
    its frozen hash, and only allowed Phase 1C implementation files differ from
    the signed post-packet recovery-ledger `HEAD` verified directly at
    dispatch. `PROGRESS.md` records the signed packet parent; the ledger is
    proved as current `HEAD`, a signed direct child that changes only the clean
    `PROGRESS.md`, while both that file and the closure packet are proved clean
    against signed `HEAD` in the index and worktree. The listed hashes and
    aggregate recipe prove that the
    rejected implementation content was unchanged while the packet and ledger
    checkpoints were added; `b083ceb` is provenance for that dirty content,
    not the final diff/staging comparison base.
  - Both entries obey exact envelope/source precedence; Guide body is UTF-8
    validated but otherwise opaque; source policy uses exhaustive CST contexts,
    not a pseudo lexer; valid block scalar punctuation and `a:\n` implicit
    empty are accepted correctly.
  - Presentation retains rich records only through node 20,000 and
    descriptor-only tail thereafter under every checked allocation bound.
    The production Saphyr reducer owns all pending buckets, guard comparison,
    tail traversal, borrowed-text/Uninspectable behavior, trace dropping, and
    14-stage selection without resynchronization or synthesized failure.
  - Numeric precharge precedes result-sized allocation; all `S`/`C`/`R`, digit,
    mirror, builder, and active overflow scopes are exact. Every builder error
    maps correctly and no partial semantic state survives.
  - Reconciliation is one monotonic eight-dimension zip; entries are a
    contiguous proven prefix with no semantic-suffix IDs; suffix facts are
    semantic; all node/tag/payload/ancestor/sibling/entry ranges and rebases are
    proved. Comments use the required parent-bucket linear algorithm and pass
    the exact checked work counter with natural ambiguous cases.
  - All fault controls exercise production continuations; no `forced_issue`,
    toy reducer, ignored property result, public hook, or non-test seam remains.
    The complete fixture/failure/limit/read-only/precedence/tail/property/
    allocation/dependency/API/rustdoc matrix is named and green.
  - Focused boundary checks, full pnpm/Cargo/Tauri gates, no-drift/absence/
    source audits, bootstrap, whitespace, and final actual-diff inspection pass
    on the installed toolchain. Both independent reviews pass with no unresolved
    blocker/high/medium finding. Evidence exactly matches the final code and
    outputs without content disclosure.
- **Expected commit boundary:** After the Orchestrator inspects every allowed
  path relative to the signed post-packet recovery-ledger dispatch `HEAD`,
  reruns the complete gate, and receives both independent passes, it
  stages one coherent signed Phase 1C implementation commit containing the
  already-resolved exact manifest/lock delta, corrected crate-private parser,
  narrow test-only builder bridge, complete private tests/fixtures, six API
  negatives, and both evidence files. It stages no task packet,
  `PROGRESS.md`, generated TypeScript, unrelated file, or `x`. It runs
  `git diff --cached --check`, inspects the exact staged name/status/stat/diff,
  commits, and verifies the signature. Only afterward does it update
  `PROGRESS.md` in a separate signed recovery-ledger commit naming the verified
  implementation hash and the exact Phase 1D Architect/xhigh gate.
- **Conditions requiring escalation:** Stop without guessing if any frozen
  hash/signature/base differs; the reviewed closure-packet checkpoint or its
  separate recovery-ledger checkpoint is absent or unsigned, `PROGRESS.md`
  does not name the exact packet hash, dispatch `HEAD` is not a signed direct
  child of that packet commit changing only `PROGRESS.md`, either that ledger
  file or this packet differs from signed `HEAD` in the index or worktree, or
  the dirty implementation differs from the recorded hash recipes; another writer
  overlaps an allowed path; the
  accepted dependency graph drifts; a pin/feature/API/event/`SyntaxKind` differs
  from audited source; ADR 0013 conflicts internally or proves infeasible; the
  exact source/pending/tail/scalar/reconciliation/comment algorithm cannot be
  implemented within its bounds; a natural witness contradicts the accepted
  reachability class; a parser panics, fails to terminate, discloses content,
  or requires recursion/unsafe/unmetered work; production-backed testing would
  require a wider hook or changed constant; a builder active scope cannot be
  observed without changing Phase 1B authority; a generated contract or public
  API changes; a required command fails outside allowed paths; or a review
  finds unresolved correctness/security risk. Any proposed signature,
  visibility, trait, ownership, limit, precedence, grammar, allocation,
  malformed/read-only, dependency, phase-authority, filesystem, or mutation
  change requires Architect/xhigh plus Security Reviewer/xhigh before work
  resumes. Preserve evidence and ask the Orchestrator; never reset or discard
  the partial implementation.
