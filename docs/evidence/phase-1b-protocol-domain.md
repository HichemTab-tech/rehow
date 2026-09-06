# Phase 1B protocol/domain evidence

Verified on 2026-09-06 with the installed toolchain: Rust/Cargo 1.98.0, Node
26.2.0, and pnpm 11.20.0. No runtime or package was installed or switched.

## Dependency and lock review

`rehow-core` adds only the exact ADR 0012 edges: production `base64` 0.23.1
with `std` and defaults disabled, `time` 0.3.55 with `std`, `formatting`, and
`parsing`, and `uuid` 1.26.0 with `std` and defaults disabled; test-only
`serde_json` 1.0.151 with `std` and defaults disabled and `trybuild` 1.0.120
with defaults disabled. The lock delta adds `base64` 0.23.1 (checksum
`ac07cdecf99051d9a5238b80f35af32cdeba5b336e55d957b318b50137e18da5`),
`trybuild` 1.0.120 (checksum
`1e605bf6b39357663d8ba4e984f8be8da8df6bb32e81031d6889024ea8fd68e4`),
and its `target-triple` 1.0.1 support package (checksum
`c3a6bfce3d99adfa72d24750a61f782f3036a81e7f86d8841ee1326deaebd171`).
All other resolved packages were already present transitively in the signed
Phase 1A lockfile.

`cargo tree -e features -p rehow-core --locked` produced 184 lines. Its runtime
graph selects only Base64 `std`/`alloc`, time `std`/`formatting`/`parsing`, UUID
`std`, Serde, and ts-rs; the separately shown dev graph contains `serde_json`,
`trybuild`, and compiler-test support. There is no core-selected UUID RNG,
`getrandom`, Base64 `simd-unsafe`, time `local-offset`, YAML parser, or BLAKE3.
The 2,019-line `cargo tree -e features --workspace --locked` truthfully shows
existing Tauri-driven UUID default/v4/rng and `getrandom` feature unification.
A source deny-list search for UUID/random generation, clock reads, BLAKE3, YAML
parsers, and dependency re-exports returned no production matches.

`command -v cargo-audit` returned no path. No advisory tool or package was
installed, so no advisory result is claimed. ADR 0012's registry/RustSec review
remains the available advisory evidence; a later environment with `cargo audit`
should repeat the locked scan.

## Protocol, API, and generated-contract evidence

- `ErrorDto` uses a bounded, field-order-independent custom map visitor with
  seven exact top-level fields. Its typed `RawDetails` decoder has an exhaustive
  36-code dispatch, per-code masks, concrete non-null field values, fixed
  entity-kind/ID correlation, and `IgnoredAny` only for immediate unknown
  detail fields. No recursive generic wire-value representation remains.
- The 36 golden JSON fixtures round-trip and match every private producer
  factory. Tests mutate every fixture for wrong types and cross-code fields and
  cover missing, null, duplicate, unknown, retryability, entity, location,
  bounded-list, integer, message/recovery, and Git-reason behavior. Unknown
  immediate detail fields are dropped; errors do not reflect hostile values or
  field names.
- Every public protocol deserialization entry point normalizes parser, shape,
  scalar-type, validation, and nested-item failures to the fixed
  `ValueConstructionError` display. Regressions cover primitive wrappers,
  closed enums, portable entities, bounded nested lists, envelope fields, and
  typed details and prove that hostile strings and numbers do not appear.
- Public detail structs derive TypeScript only and keep private fields. Their
  private runtime data owns `Clone`/`Eq`/`Serialize`; consumers cannot clone,
  compare, serialize, deserialize, or construct detail values directly.
  Unknown-YAML value/tree records remain `Clone` only. The four closed YAML kind
  enums expose `Clone + Copy + Eq + PartialEq + Ord + PartialOrd + Hash`;
  `ValueConstructionError` has `Clone + Copy` but no equality/order/hash traits,
  and `UnknownYamlBuildError` exposes none of those copy/comparison traits.
  `PublicText` exposes only the approved `Clone + Eq + PartialEq` set; compile
  failures prove `Ord`, `PartialOrd`, and `Hash` remain absent.
- TypeScript shapes are produced from private derived tagged, untagged, and
  flattened ts-rs markers. The only composed exported override is the approved
  `ErrorEnvelope & ErrorVariant` intersection; there are no handwritten full
  object or union schema strings. The checked output has 73 declarations: 25
  string aliases, seven closed aliases, correlated portable entity, bounded
  list, 36 detail aliases, envelope, exhaustive variant, and `ErrorDto`. No
  helper marker, scaffold smoke, or unknown-YAML declaration leaks.
- Two fresh generations were byte-identical at SHA-256
  `156c56374875090606dde45f6141b8f2a22a8eeeb3d1359423edf696893d3511`.
  A deliberate one-byte drift at byte 39 made `pnpm contracts:check` exit 1;
  `pnpm contracts:generate` restored the output and the check passed.
- The strict TypeScript fixture exhaustively narrows all 36 codes and rejects
  invalid correlation, retryability, decimal representation, nullable
  optionals, unpaired source locations, and an unknown-YAML import.
- Trybuild passes one expanded external-consumer fixture and 18 compile-fail
  fixtures. They cover the documented parse/accessor/builder/Serde/TS surface
  and reject private representations, fields, construction, generators,
  dependency re-exports, catalog/detail/ErrorDto construction, unknown-YAML
  construction/Serde/TS/Default/equality/partial extraction, configurable
  limits, `PublicText` deserialization, and `PlanToken` display.
- Protocol value tests cover every wrapper's string-only Serde, UUID v7
  version/variant/payload boundaries, timestamp fractions 0–10 and calendar
  boundaries, canonical decimals/digests/tokens, display/path scalar and UTF-8
  escaping, DNS/IP limits and legacy numeric host aliases, and every bounded
  list consistency state. `PlanToken` decoding/re-encoding uses fixed-size
  buffers and checks canonical RFC 4648 spelling.
- The UUID grammar table rejects every version nibble other than 7 (`0`–`6`
  and `8`–`f`) using an otherwise canonical lower-hyphen RFC-variant value.
  Every wrapper separately passes the shared parser's strict string-Serde
  matrix, including v7 payload and variant boundaries.
- All 39 `SchemaFieldPath` catalog spellings have exact accessor and string
  round trips. The other four constructed consumer tokens cover empty, maximum
  64-byte, lowercase/digit/underscore grammar and reject uppercase,
  digit-leading, trailing/double underscore, hyphen, 65-byte, and non-string
  inputs. Tables round-trip every wire value of the six closed discriminants
  and all 36 standalone `ErrorCode` values, with unknown and wrong-type
  rejection for each type.
- The bounded-list wire matrix accepts `0/0`, `32/32`, `33/32`, and `40/32`.
  A counted `Bomb` item deserializer proves rejection occurs before item 33 is
  deserialized. Display evidence includes the 3,072-byte `SafeGitDisplay`
  boundary, the 3,072/3,073-character escaped relative-path boundary, every
  prohibited Unicode class, and DNS 63/64-label and 253/254-host boundaries.
- A table naming every required and optional detail key for all 36 codes drives
  removal, null, wrong-type, and duplicate mutations. It also checks paired
  locations, all seven top-level types and duplicates on every fixture, safe
  noncatalog copy preservation, and all producer-side correlation/count
  rejection branches.

## Unknown-YAML meters and semantics

The unit and integration suites cover all scalar/value accessors, complex keys,
mapping-order-insensitive duplicate semantics, sequence-order distinction,
integer/float and signed-zero/NaN distinctions, all 19 builder error kinds,
poisoned follow-up, structural error precedence, and exact/one-over depth,
collection, scalar, numeric, and node limits. Exact 2,048-item sequences and
depth-16 documents close and finish successfully, and cloned numeric values are
charged per occurrence. Private unit tests additionally exercise
checked-overflow hooks and natural (unseeded) meter boundaries:

- a root key, sequence, 127 strings of 65,536 bytes, and one string of 64,374
  bytes reach exactly 8,388,608 retained-encoding bytes through public builder
  events; the next empty-string event is rejected;
- a five-scalar-key nested map charges 66 units and a reverse 256-key map using
  `"a" * 13,106 + index` charges `29,218 + 2,558 * 13,106`, reaching exactly
  33,554,432 comparison units through public builder events; closing the root
  requests the next comparison and is rejected;
- public builder events construct exactly 20,000 nodes and reject node 20,001;
- the fixed two-entry stable merge plus adjacent duplicate scan charges exactly
  40 comparison units; an eight-key reverse map separately proves merge widths
  1, 2, and 4 charge 104 each and the adjacent scan charges 182, totaling 494.

The over-limit and structural-error tests prove poisoned state and no partial
document extraction. The natural meter vectors complete in the ordinary core
test run (about half a second on this machine).

## Resource and artifact evidence

The pre-Phase1B release artifacts from the signed scaffold were 50,808 bytes for
the selected `rehow-core` rlib, 450,408 bytes for `rehow-mcp`, and 18,812,854
bytes for `librehow_desktop.rlib`. After the final
`cargo build --release --workspace --locked`, newest-artifact selection reports
2,890,726 bytes for `rehow-core`, 450,408 bytes for `rehow-mcp`, and 18,809,068
bytes for `librehow_desktop.rlib`. These are review evidence, not thresholds;
the inert adapters do not yet link core protocol operations.

## Verification commands

The exact final integrated sequence passed:

- installed version checks and `pnpm install --frozen-lockfile` (already up to
  date)
- `pnpm contracts:generate` and `pnpm contracts:check`
- `pnpm check`
- `pnpm test` — Rust core 13 unit, 18 error-contract, 13 protocol-value, one
  trybuild harness (one positive plus 18 negative cases), and 11 unknown-YAML
  tests; desktop 1 and MCP 1; Vitest 1
- `pnpm test:integration`
- `pnpm build` — TypeScript and Vite production build passed
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo test --workspace --all-features --locked`
- `cargo test --locked -p rehow-core --test public_api`
- both locked feature-tree commands described above
- `cargo build --release --workspace --locked`
- `python3 scripts/check_bootstrap.py` — 17 required files, 12 ADRs, 8 pinned
  agents
- `git diff --check`

The Orchestrator repeated this aggregate packet gate after the final test-only
protocol evidence expansion and independent review; all commands passed with
13 protocol-value tests (56 Rust test functions total, including the trybuild
harness).

All filesystem reads in Phase 1B tests are confined to checked-in golden/API
fixtures, compiler/build outputs, and ordinary Cargo/pnpm caches. No test opens
a Workspace, Git repository, clock, randomness source, network resource, or
personal content.
