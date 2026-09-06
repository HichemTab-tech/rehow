# Task Packet: Phase 1B protocol and parser-neutral domain values

- **Objective:** Implement the accepted Phase 1B Rust protocol boundary and
  parser-neutral unknown-YAML semantic construction boundary. A successful
  result has canonical parse-only value types, the exhaustive 36-code
  `ErrorDto` contract, deterministic generated TypeScript, and a bounded,
  poisoning unknown-YAML event builder, with all accepted/rejected wire,
  resource, correlation, and public-API evidence passing. It performs no
  Workspace I/O and grants no clock, entropy, hashing, parser, Git, Tauri, or
  MCP authority.
- **Required reasoning level:** low. Use the project Worker role
  (`gpt-5.6-sol`, low) exactly. ADRs 0011 and 0012 have frozen the architecture,
  dependency set, public API, wire spellings, limits, work accounting, and phase
  boundaries. The Worker implements those decisions and must not replace them
  with a simpler model, infer aggregate behavior, or widen an API.
- **Prerequisites:** Signed decision commit `f35302d`, its following progress
  ledger `2995d38`, and the Orchestrator's signed checkpoint containing this
  reviewed packet plus the `ErrorConstructionError` visibility clarification
  must be present; `PROGRESS.md` names that exact packet checkpoint before
  dispatch. Read `AGENTS.md`, `PROGRESS.md`, this packet, every document below,
  and the actual files before editing. Run
  `git status --short --branch` and `git log --oneline -8`; confirm that no other
  writer owns an allowed path. The untracked root executable `x` is unrelated
  user state: do not execute, read, modify, stage, remove, or mention it in an
  implementation commit. Use the installed Node 26.2.0 and pnpm 11.20.0 and
  Rust/Cargo 1.98.0; do not install or switch runtimes. Record pre-change release
  artifact sizes before editing as required below. If any prerequisite differs,
  stop and report the exact difference to the Orchestrator.
- **Allowed files or subsystem:** The Worker has exclusive ownership only of:
  - `crates/rehow-core/Cargo.toml` and `Cargo.lock`;
  - `crates/rehow-core/src/lib.rs` only to replace the empty inline `domain`
    module with `pub mod domain;`; its other module declarations remain intact;
  - `crates/rehow-core/src/domain.rs` and private implementation files beneath
    `crates/rehow-core/src/domain/`, limited to `yaml_value.rs` and narrowly
    named private helper modules if the single file becomes unreviewable;
  - `crates/rehow-core/src/protocol.rs` and private implementation files beneath
    `crates/rehow-core/src/protocol/`, limited to `construction.rs` and narrowly
    named private Serde/contract helpers;
  - `crates/rehow-core/examples/export_contracts.rs`;
  - Rust integration/API tests under `crates/rehow-core/tests/`, including
    `protocol_values.rs`, `error_contract.rs`, `unknown_yaml.rs`,
    `public_api.rs`, and `tests/ui/**` trybuild sources and `.stderr` baselines;
  - checked-in JSON fixtures under
    `crates/rehow-core/tests/fixtures/error-contract/v1/`, exactly one golden
    file per error code plus explicitly named malformed fixtures when sharing a
    programmatically generated mutation would hide a requirement;
  - `packages/contracts/src/generated.ts` (generator output only) and
    `packages/contracts/src/error-contract.type-test.ts`;
  - `docs/evidence/phase-1b-protocol-domain.md`, containing commands, dependency
    feature evidence, lockfile review, and before/after artifact sizes only.

  Do not edit root `Cargo.toml`, JavaScript manifests/lockfiles, scripts,
  adapters, UI, capabilities, agent configuration, accepted specs/ADRs,
  `PROGRESS.md`, or any other file. The Orchestrator owns progress and commits.
  Do not stage or commit.
- **Relevant specifications and ADRs:**
  - `AGENTS.md`: invariants, Rust-core authority, security boundaries, testing,
    orchestration, and commit ownership.
  - `docs/implementation-plan.md`: Phase 1 milestone 2, Phase 1 gate, dependency
    policy, and the explicit Phase 1B/1C/1D/1E division.
  - `docs/architecture.md`: core/adapter and domain/protocol separation,
    Rust-authoritative contracts, disclosure policy, and review triggers.
  - `docs/content-model.md`: unknown-YAML semantic shapes and limits,
    verification-hash lexical-only boundary, and prohibition on aggregate
    implementation before Phase 1D.
  - `docs/error-contract.md`: exact wire primitives, bounded list, all 36 codes,
    detail properties, disclosure policy, storage/protocol separation, and
    adapter mapping.
  - `docs/testing-strategy.md`: Rust/contract layers and mandatory bounded-list,
    hostile, feature, and cross-language evidence.
  - `docs/threat-model.md`: hostile content, resource exhaustion, secret/path
    disclosure, dependency, and review boundaries.
  - Accepted ADR 0011, `docs/adr/0011-bounded-yaml-semantics-and-lossless-span-edits.md`:
    canonical unknown values, key equality/encoding, stable merge algorithm,
    exact meters, and Phase 1B versus later-parser authority.
  - Accepted ADR 0012, `docs/adr/0012-phase-1b-protocol-construction-and-dependencies.md`:
    every dependency, public/private API, grammar, catalog, factory, Serde,
    TypeScript, builder, test, and non-goal below. ADR 0012 is normative where
    this packet abbreviates a closed catalog or fixed English string.
- **Exact required behavior:**
  1. Add only these exact `rehow-core` edges; retain the existing exact Serde and
     ts-rs pins, do not create a Workspace dependency table, and regenerate
     `Cargo.lock` through Cargo rather than editing it:

     ```toml
     [dependencies]
     base64 = { version = "=0.23.1", default-features = false, features = ["std"] }
     serde = { version = "=1.0.229", features = ["derive"] }
     time = { version = "=0.3.55", default-features = false, features = ["std", "formatting", "parsing"] }
     ts-rs = "=12.0.1"
     uuid = { version = "=1.26.0", default-features = false, features = ["std"] }

     [dev-dependencies]
     serde_json = { version = "=1.0.151", default-features = false, features = ["std"] }
     trybuild = { version = "=1.0.120", default-features = false }
     ```

     Add no other direct dependency or feature. Production code must not use or
     re-export UUID generation, random/entropy, current time/local offset, RFC
     2822, unsafe Base64 SIMD, JSON/YAML parsing, BLAKE3, or dev dependencies.
  2. Replace only the empty inline `domain` declaration with the external public
     `domain` module. `domain::yaml_value` is private. Re-export from `domain`
     exactly the public types and methods frozen by ADR 0012: canonical YAML
     integer/float types and kind enum; unknown metadata document/entry, YAML
     value/mapping-entry/scalar and kind enums; `UnknownYamlEvent`;
     `UnknownYamlDocumentBuilder`; and the two public build-error types. Keep all
     fields and all representation enums private. The completed tree is the
     complete generic root, not an extension bag, and Phase 1B does not classify
     known/reserved root keys. Define the one shared public
     `ValueConstructionError` in `domain` and have protocol primitive public
     signatures refer to that type; do not duplicate or re-export it from
     `protocol` under a second public path.
  3. Implement `CanonicalYamlInteger::new(negative, digits: String)` and its two
     accessors exactly as ADR 0012: nonempty ASCII decimal digits; no leading
     zero except `0`; at most 65,536 digits; negative zero normalized positive;
     no retained rejected input. Implement `CanonicalYamlFloat::finite`, the
     three special constructors, its kind/sign/coefficient/exponent accessors,
     and finite normalization constraints exactly as ADR 0012/content model. A
     finite coefficient is nonempty canonical decimal, has no leading/trailing
     zero unless exactly `0`, and coefficient plus exponent digits is at most
     65,536; zero coefficient requires zero exponent but retains the float sign.
     These constructors return `ValueConstructionError`, not builder errors.
  4. Implement the exact move-only nine-variant `UnknownYamlEvent` and exact
     builder methods `new`, `push_event`, and consuming `finish_document` from
     ADR 0012. Do not add `Default`, reset, iterator, convenience, configurable
     limit, unfinished-value, or representation access. The first accepted
     event is the root `BeginMapping`; root keys must be completed strings;
     nested keys may be complex; entry/sequence order is retained; key/value
     alternation and matching ends are mandatory; exactly one completed root is
     required.
  5. On every builder failure, replace state with `Poisoned` before returning,
     destroy partial frames/root/key caches, and make all later push/finish calls
     return `Poisoned`. Implement the exact 19 `UnknownYamlBuildErrorKind`
     variants, exact shared Display text, `kind()`, precedence, finish behavior,
     and no-content error storage from ADR 0012. Validate an event before
     attachment; use checked arithmetic and return `ArithmeticOverflow` before
     state mutation. Unit-only private meter seeding/hooks may exist under
     `#[cfg(test)]` to exercise checked overflow; they must not be public or
     available to integration/trybuild callers.
  6. Enforce exactly: depth 16 with root depth 1; 20,000 nodes; 256 completed
     pairs per mapping; 2,048 completed children per sequence; 65,536 UTF-8
     bytes per string or numeric scalar; 81,920 retained numeric digits per
     document; 8,388,608 cached canonical-key encoding bytes; and 33,554,432
     comparison byte-read units. Equality is accepted and the next unit/event
     fails without a result. Count starts/scalars as nodes and only completed
     children/pairs as entries. Reject Unicode noncharacters in event strings.
     Charge each occurrence of cloned canonical numbers again.
  7. Implement duplicate-key semantics and work accounting exactly, not merely
     equivalently: injective recursively length-framed canonical key encoding;
     signed floating zero normalized throughout key context; one bottom-up
     cached occurrence encoding; mapping pair order independent and sequence
     order dependent; scalar types distinct; all NaNs equal; integer and float
     distinct. For every nested mapping use stable bottom-up merge sort with
     widths `1,2,4,...`, key then value lexicographic comparison, left record on
     equality, followed by an adjacent-key duplicate scan. Charge one cached
     byte before each append, two comparison units per paired byte read, and no
     unit for cached-length comparison after an equal prefix. Never use standard
     sort, a hash map, recursive all-pairs equality, or a public semantic Eq.
  8. Expose exactly the read-only accessor API block in ADR 0012. All `as_*`
     accessors return `None` on a kind mismatch and never move/mutate children or
     expose caches. Implement only the exact traits ADR 0012 assigns to
     canonical numbers, completed trees, events, and errors. In particular,
     unknown values/events have no Debug/Serde/TS/Default/Hash/Ord, completed
     trees have no ordinary semantic equality, and no unchecked construction
     path is reachable outside `domain::yaml_value`.
  9. Replace `ContractGenerationSmoke` with the protocol v1 types; no smoke type
     or smoke declaration remains. Implement the distinct parse-only UUIDv7
     wrappers, `RecentWorkspaceId`, `Timestamp`, decimal wrappers, digest
     wrappers, `PlanToken`, `PortableEntityId`, closed discriminants, safe
     display wrappers, `PublicText`, `BoundedList<T>`, `SourceLocation`, all
     named detail structs, `ErrorCode`, `ErrorDto`, and their exact ADR 0012
     methods/traits. `protocol::construction`, emitting catalog enums, marker
     types, unchecked constructors, error kind, and representations are private.
     Re-export the named public protocol types from `protocol`; do not re-export
     dependency types or private helpers. Public structs have private fields.
     `ErrorDto`, its public detail structs, `SourceLocation`, and closed
     discriminants receive no unapproved getter or constructor: their Phase 1B
     observation surface is custom Serde, TS generation, pattern matching on a
     public closed enum where applicable, and the explicitly named traits only.
     Add no public trait implementation, constructor, conversion, accessor, or
     alias beyond ADR 0012.
  10. Define one opaque public `domain::ValueConstructionError` shared by all primitive
      parse/construction failures. It is unit-like, `Copy`, has Display exactly
      `invalid canonical value`, implements `std::error::Error`, retains no
      input/cause/kind, and has fixed non-content Debug. Primitive wrappers must
      use manual/custom Serde rather than permissive dependency Serde.
  11. Parse each ordinary ID from exactly lowercase canonical hyphenated UUIDv7,
      RFC variant, any 48-bit millisecond payload; parse recent IDs after exactly
      `recent:v1:`. Expose `FromStr`, `TryFrom<&str>`, Display, custom string
      Serde, and only the assigned comparison/hash traits. Never generate an ID
      or expose UUID/bytes/components. `PortableEntityId` has only the four safe
      typed constructors and custom strict correlated Serde.
  12. Parse and compare `Timestamp` exactly as ADR 0012: uppercase UTC grammar,
      years 0001–9999, valid Gregorian date, seconds at most 59, optional 1–9
      digit fraction with no trailing zero, at most 30 bytes, exact canonical
      reformat, chronological `Ord`, string-only Serde. Expose no component or
      current-time constructor and make no clock call.
  13. Implement public infallible decimal `new` constructors plus canonical
      string parsing/Serde for full `u64` and `i64`; numbers, `+`, leading zero,
      whitespace, `-0`, and overflow are rejected as applicable. Implement
      lexical-only `Revision`, `Snapshot`, and `VerificationHash` with exact
      prefix plus 64 lowercase hex and private `[u8;32]`; manually decode/encode,
      expose no digest constructor, and compute no hash. Implement parse-only
      `PlanToken` with exact prefix, alphabet, length, permitted final-character
      class, decode to exactly 32 bytes, and byte-identical URL-safe-no-pad
      re-encode. It has custom string Serde, no Display/text/byte accessor,
      ordering, generation, binding, expiry, or single-use behavior.
  14. Implement the five constructed token types and all exact Phase 1B
      catalogs from ADR 0012. Public consumer deserialization accepts the exact
      schema-path catalog or syntactically valid token as specified; producer
      factories take only private closed catalog variants. Implement
      `SafeRelativePath::parse_wire`, `SafeGitDisplay::parse_wire`, and
      `SafeHost::parse_wire` with their exact canonical decode/re-encode,
      prohibited-scalar, noncharacter, percent, length, path-segment, DNS, IP,
      and host rules. IPv4-mapped IPv6 accepts Rust 1.98 canonical
      `::ffff:192.0.2.128` and rejects `::ffff:c000:280`. They validate display
      strings only and expose no decoded OS/Git input.
  15. Keep `PublicText` producer construction private and limited to the exact
      fixed message/recovery/five-Git-reason catalog. It has `as_str`, string
      serialization, and TS `string`, but no public constructor or standalone
      Deserialize. Error consumer deserialization privately accepts/preserves
      any nonempty safe text through 256 Unicode scalars; it rejects every
      prohibited scalar/noncharacter. Safe noncatalog copy and Git reasons must
      round-trip exactly but never become producer construction authority.
  16. Implement all 36 exact `ErrorDto` factories and their argument order in
      private `protocol::construction` with `pub(super)` visibility, its narrowly
      scoped `#![allow(dead_code)]`, and no broader lint allowance. Each returns
      `ErrorConstructionError`, which is itself `pub(super)` solely to keep the
      signature visibility consistent, but is an opaque braced struct with a
      private unit field and no constructor outside `construction`; factory
      callers cannot choose code, retryability, copy, Git sanitized reason, or
      arbitrary catalog text.
      Enforce every additional correlation/count invariant in ADR 0012. Exactly
      five codes are retryable: `WORKSPACE_NOT_FOUND`, `WORKSPACE_UNAVAILABLE`,
      `IO_BUSY`, `INDEX_NOT_READY`, and `GIT_AUTH_REQUIRED`.
  17. `BoundedList<T>::try_new(items, total_count)` is its only public
      construction/deserialization path. Retain at most 32 items and require
      `items.len() == min(total_count,32)`; derive `truncated` and expose only its
      slice/count/flag accessors. Implement the accepted 0/0, 32/32, 33/32,
      40/32 behavior and reject every shorter, over-retained, or inconsistent
      representation.
  18. Implement strict custom `ErrorDto` Serde exactly as ADR 0012: exactly seven
      unique, required, non-null top-level keys; literal version 1; exact code;
      correlated detail and retryability; safe consumer copy; exact nested
      required/optional semantics; paired locations; all primitive/invariant
      validation. Reject unknown top-level, duplicate known fields at every
      depth, and detail keys belonging to another code. Ignore/discard only
      genuinely unknown immediate detail keys, including duplicate unknown
      keys; do not re-emit them. All other nested objects reject unknown keys.
      Serialization emits no unknown/null/undocumented property and never
      switches on `is_human_readable`. Error Debug shows only code/request ID.
  19. Generate TypeScript from Rust only. Update the export example so stdout is
      deterministic and emits declarations in the exact ADR 0012 order and
      shape: 25 named string aliases; seven closed aliases; correlated
      `PortableEntityId`; `BoundedList`; 36 detail aliases; `ErrorEnvelope`;
      exhaustive 36-arm `ErrorVariant`; and `ErrorDto = ErrorEnvelope &
      ErrorVariant`. Use private ts-rs marker/contract types as required; do not
      hand-edit generated output or keep a second handwritten schema. Preserve
      camelCase, optional-without-null, inline source-location unions, exact
      literal version/retryability/code correlation, and the precise alias names
      and order in ADR 0012. Generate no unknown-YAML TypeScript declaration.
  20. Keep all error content disclosure-safe: no fixture/factory/debug output may
      contain raw Guide/file content, commands, Git/parser/OS output, ambient
      absolute paths, environment values, credentials, unredacted URLs,
      recovery-object paths, or secret material. The one serialized fixed
      `planToken` field is allowed only in its required confirmation fixture;
      Debug remains redacted. No product code or subject-under-test accesses a
      Workspace, Git repository, clock, randomness, network, ambient/user path,
      or user content. Test harness access is limited to the repository fixtures,
      temporary contract-drift copy, build outputs, and ordinary Cargo/pnpm
      caches explicitly required by this packet; it exercises no product
      filesystem behavior.
- **Important states and edge cases:** Exact and one-over lengths are distinct;
  canonical inputs re-emit byte-for-byte and every alternative spelling fails.
  Include minimum/maximum UUID timestamp payloads and RFC variant boundaries;
  Gregorian/leap/month boundaries and nanosecond chronological order; all
  decimal machine boundaries; digest prefix/case/length; all Base64 final
  classes and nonzero pad bits; every prohibited display scalar/noncharacter
  and percent encoding; IPv4/IPv6 mapped spelling; empty, exact, and truncated
  lists; every required/optional error property; noncatalog safe inbound copy;
  unknown detail drop; duplicate/unknown/mismatched nested fields; zero/nonzero
  correlation boundaries. For YAML construction cover never started, wrong
  root, unmatched/mismatched ends, partial key, completed root plus extra event,
  every scalar/value/complex-key shape, encounter order, every duplicate-equality
  rule, every error kind, poisoned follow-up, checked overflow, each exact limit
  and one over, and no surviving partial result. A failed event must not be
  recoverable by sending a closing event.
- **Security constraints:** Treat every parse/event/Serde input as hostile and
  enforce caps before retaining/appending. Errors and Debug never retain or echo
  rejected content. Do not add generation, ambient time/entropy, hashing,
  parser, filesystem, process, shell, Git, network, logging, or adapter behavior.
  Do not expose dependency types or private representation/caches. Do not use
  unsafe code, recursion for adversarial tree construction/comparison, standard
  sort or a hash map for semantic equality, configurable limits, arbitrary
  catalog/text constructors, or Serde derives that bypass canonical parsing.
  Unknown YAML is not a serialization/write source. No path/Git factory becomes
  reachable outside protocol fixtures. Do not inspect environment dumps,
  credentials, remotes, personal content, or paths outside the repository and
  ordinary Cargo/pnpm caches/toolchains. A dependency advisory, native/unsafe
  feature, or unexpected transitive package is a stop condition.
- **Tests to add or update:**
  - `protocol_values.rs`: table-driven accept/reject/round-trip/string-only Serde
    tests for every ID/prefix, timestamp, decimal, digest, token, constructed
    token, safe-display, host, and trait-visible behavior named in ADR 0012.
    Cover all UUID versions/invalid variant/payload boundaries; timestamp
    min/max/leap/month/fraction 0–10/offset/leap-second/order; numeric bounds;
    RFC 4648 final classes/nonzero pad bits; all display scalar/path/host cases.
  - `error_contract.rs`: load 36 golden JSON files; assert exact top-level keys,
    consumer round-trip and rejection behavior,
    all required/optional/paired properties, five Git reasons, and all list
    vectors (2/2, 40/32, 33/32, 32/32, 0/0, >32, every inconsistent count/flag).
    Programmatically mutate each fixture for missing/null/duplicate/wrong type,
    code/detail/retryability mismatch, unknown top-level, cross-code detail,
    invalid nested field, entity mismatch, and integer/string boundaries. Prove
    safe noncatalog message/recovery/Git reason preservation, invalid PublicText
    rejection, immediate unknown-detail acceptance/drop, and redacted Debug.
    Private unit tests inside `protocol`/`construction` must create all 36
    factory results and compare their serialization byte-for-byte or
    value-for-value with those same golden fixtures; those unit tests, not the
    external integration test, prove exact producer copy/details/retryability,
    every factory invariant, and all five factory-derived Git reasons. Do not
    widen factory visibility to make an integration test convenient.
  - `unknown_yaml.rs` plus private unit tests: success for every kind/accessor and
    nested complex keys; order and semantic distinction/equality vectors;
    signed zero and NaN; all 19 error kinds and poison follow-ups; exact/one-over
    depth/node/map/sequence/scalar/numeric/cached-encoding/comparison limits;
    overflow hooks; stable-merge counter vectors; adjacent duplicate scan; each
    scalar occurrence charged; and no partial extraction. Large tests must be
    deterministic and terminate within ordinary unit-test time.
  - `public_api.rs` invokes trybuild. Add positive fixtures for only documented
    parse/accessor/builder/Serde/contract APIs. Add focused compile-fail fixtures
    proving no private module/representation/field/unchecked constructor;
    direct entry/value/document construction; unknown-YAML Serde/TS/Default or
    partial extraction; configurable/skipped limits; emitting catalog/text/
    detail/ErrorDto construction; UUID/token/time/digest generation; PlanToken
    Display/text/bytes; or dependency generation API re-export. Check in stable
    `.stderr` files and normalize no host path into them.
  - `error-contract.type-test.ts`: exhaustive `switch` over all 36 codes with a
    `never` default and positive member narrowing. Add `@ts-expect-error` cases
    for wrong code/detail/retryability, nullable optionals, wrong decimal wire
    numbers, and unknown-YAML imports. The intentionally unbranded ID aliases
    are all `string`, so their kind/ID correlation is enforced by Rust
    construction and Serde tests rather than an impossible TypeScript
    nominal-type assertion. Existing strict package `tsconfig` must typecheck
    the file without configuration changes.
  - Contract generator test/evidence: two fresh generations are byte-identical,
    `pnpm contracts:check` detects a temporary one-byte drift and passes after
    generator restoration, generated declaration order/shape matches ADR 0012,
    and no `ContractGenerationSmoke`/unknown-YAML declaration remains.
  - Source/feature evidence: run a deny-list search over `crates/rehow-core/src`
    for `new_v7`, UUID RNG features/calls, `SystemTime::now`, `now_utc`,
    `getrandom`, `rand`, BLAKE3, YAML/parser imports, and forbidden re-exports.
    Record `cargo tree -e features -p rehow-core --locked` and
    `cargo tree -e features --workspace --locked` summaries. The first must show
    no core-selected RNG/getrandom, Base64 unsafe SIMD, local time, YAML,
    BLAKE3, or production dev dependency; the second must honestly record
    Tauri-driven feature unification without claiming global absence.
  - Review every newly locked package/version/source/checksum and record it in
    `docs/evidence/phase-1b-protocol-domain.md`. Record the RustSec/advisory
    command used and its result; if no advisory tool is installed, do not
    install one—record that limitation for Security Reviewer examination and
    escalate any known/advisory result rather than waiving it.
  - Before and after implementation, run the same locked release build and
    record byte sizes for `target/release/rehow-mcp`, the `rehow-core` rlib, and
    `rehow_desktop` rlib using deterministic `find`/`stat` selection. Treat sizes
    as review evidence, not an unapproved threshold. If multiple hashed artifacts
    exist, clean only via the normal non-destructive Cargo clean command after
    Orchestrator approval or select the artifact produced by the current build;
    never delete arbitrary paths.
- **Commands to run:** Run from repository root, with installed runtimes only.
  Capture command/result summaries in the evidence file. Before editing, run the
  version/status/log checks and `cargo build --release --workspace --locked`,
  then record baseline sizes. After editing the exact dependency table, run
  `cargo check -p rehow-core` once without `--locked` to let Cargo add the new
  exact-pinned packages and their transitives to `Cargo.lock`; inspect that
  delta immediately. If an already locked package must be moved, use
  `cargo update` only with exact `-p ... --precise ...` arguments; never broadly
  upgrade the lockfile. Every subsequent Cargo command uses `--locked`. The
  acceptance sequence is:

  ```sh
  node --version
  pnpm --version
  rustc --version
  cargo --version
  pnpm install --frozen-lockfile
  pnpm contracts:generate
  pnpm contracts:check
  pnpm check
  pnpm test
  pnpm test:integration
  pnpm build
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  cargo test --workspace --all-features --locked
  cargo tree -e features -p rehow-core --locked
  cargo tree -e features --workspace --locked
  cargo build --release --workspace --locked
  python3 scripts/check_bootstrap.py
  git diff --check
  git status --short --branch
  ```

  Also run `cargo test --locked -p rehow-core --test public_api` explicitly so
  compile-pass/fail evidence cannot be hidden in aggregate output. Run the
  source deny-list search described above and inspect results manually; a text
  match in a negative test/comment is not a production violation. Exercise
  contract drift against a temporary copy or a deliberate one-byte generated
  file edit followed immediately by `pnpm contracts:generate`; do not restore
  with Git checkout/reset. Confirm the final generated file is clean relative to
  a fresh generation. Do not run a system package manager or install Node/pnpm,
  audit tools, test tools, or dependencies outside the exact Cargo manifest.
- **Required reviews and artifacts:** The Worker supplies the complete diff,
  exact command results, all 36 JSON fixtures, checked-in trybuild stderr,
  generated TypeScript and TS negative/exhaustiveness fixture, the feature-tree
  outputs/summaries, source deny-list result, lockfile package review, advisory
  status, and before/after release sizes. The Orchestrator inspects actual files,
  generated declarations, lockfile delta, public API, tests, and output rather
  than trusting the summary. Independent Reviewer/high approval is mandatory.
  Security Reviewer/xhigh approval is mandatory for canonical input handling,
  disclosure/redaction, dependency/features, resource meters, poisoned state,
  and private construction authority. No Design Reviewer/screenshots are needed
  because no UI changes are allowed. Architect/xhigh must review any proposed
  departure from the accepted API/algorithm/phase boundary before work resumes.
- **Explicit non-goals:** No ID/timestamp/token/digest generation; no clock,
  entropy, random, hash, approval, plan store, expiry, replay, request middleware,
  incident service, or logging. No Manifest, Workspace, Category, Guide,
  Attachment, Reference, lifecycle/status aggregate, verified-content hash
  computation/binding, application use case, or typed known-field binder. No
  YAML/Markdown source parsing, resolution, envelope/CST/presentation artifact,
  serializer/emitter/span edit, parser dependency from ADR 0011, filesystem,
  storage, confinement, revision/snapshot computation, recovery, Git, search,
  MCP/Tauri command, adapter, UI, capability, CSP, package script, runtime,
  JavaScript dependency, or product documentation change. No raw OS path/Git
  output/URL sanitizer and no application-reachable path/Git error factory.
  Unknown YAML has no Serde/TS or independent extension-bag authority. Do not
  update progress, commit, push, amend, publish, or touch `x`.
- **Definition of done:**
  - Only allowed paths changed; exact five dependency edges/features and only
    their required lockfile delta exist; the lock/advisory/size evidence is
    complete and truthful.
  - Every public/private type, method, trait, grammar, catalog, error invariant,
    fixed copy, retryability value, strict/forward-compatible Serde boundary,
    and TypeScript declaration matches ADR 0012 exactly.
  - Unknown-YAML construction uses the exact event API, complete mapping root,
    private representations, poisoning/no-partial-result behavior, deterministic
    semantic equality encoding/sort, and exact resource accounting from ADRs
    0011–0012; every error kind and limit boundary is proven.
  - No generation/clock/entropy/hash/parser/storage/Git/adapter authority exists,
    no unknown-YAML Serde/TS exists, and external-crate compile tests prove the
    intended negative API surface.
  - All 36 golden ErrorDto fixtures and hostile mutations pass; noncatalog safe
    inbound copy is preserved; unsafe content/correlation/count states fail;
    Debug and fixtures meet disclosure rules.
  - Generated TypeScript is deterministic, Rust-owned, exactly correlated and
    exhaustive; strict tsc positive/negative cases and drift checks pass.
  - Every command above passes with Node 26.2.0, pnpm 11.20.0, and Rust 1.98.0;
    no required test is skipped/flaky; bootstrap and whitespace checks pass.
  - Reviewer/high and Security Reviewer/xhigh report no unresolved
    blocker/high/medium finding. The Orchestrator inspects/stages the coherent
    implementation and records verified state separately in `PROGRESS.md`.
- **Expected commit boundary:** One coherent Phase 1B implementation commit:
  exact core dependency/lock changes, domain/protocol implementation, Rust and
  TypeScript fixtures/tests, deterministic generated contract, and the Phase 1B
  verification evidence file. Exclude `PROGRESS.md`, `x`, adapters, later-phase
  work, and unrelated changes. The Orchestrator owns staging/signing/commit and
  should use a message such as `feat: implement phase 1b protocol domain`; it
  must not amend prior commits or push.
- **Conditions requiring escalation:** Stop without guessing if any accepted
  grammar/API/catalog/correlation/trait/TypeScript shape or ADR interaction is
  ambiguous; ts-rs 12.0.1 cannot produce the exact correlated output without a
  new dependency or handwritten generated contract; exact builder meters or
  stable-merge counts cannot be implemented/tested as specified; Rust 1.98
  formats mapped IPv6 differently from the accepted vector; another writer
  overlaps an allowed file; an existing user change would be overwritten; an
  additional file/dependency/feature/lint allowance/public API is needed; Cargo
  resolves a different exact direct version or an unexpected/advisory package;
  feature unification grants an observable forbidden core API; a fixture would
  need secret/raw/path content; a test requires filesystem/Git/clock/random/
  network/user content; a required command fails outside a strictly packet-local
  correction; or completing the work would cross into Phase 1C–1E. Report exact
  evidence and request Architect/Orchestrator direction; do not weaken a limit,
  test, security rule, contract, or phase gate.
