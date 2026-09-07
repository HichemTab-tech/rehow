# Phase 1C dependency evidence

Date: 2026-09-06 (Africa/Algiers)

Status: **dependency audit passed; changed graph compiled and final graph rechecked**.
Before any changed dependency or project source was compiled or executed, the
local source/lock/tree/current-registry audit completed. After the shell could
not resolve the OSV API host, the Orchestrator retrieved OSV's official current
crates.io data dump and the current RustSec advisory database through their
documented public sources. Security Reviewer/xhigh independently reproduced the
graphs and source facts, verified the exact-version advisory results, and
opened the gate with no blocker/high/medium finding. The first changed-graph
command was exactly `cargo check -p rehow-core --locked`; it exited 0.

## Baseline and toolchain

- Signed prerequisites: `git verify-commit df067b2`,
  `git verify-commit f39d7ab`, and `git verify-commit c98ab13` all exited 0
  with a good signature from the configured project signing identity.
- Host: `x86_64-unknown-linux-gnu`.
- Node: `v26.2.0`; pnpm: `11.20.0`; rustc:
  `1.98.0 (88d9e12ae 2026-08-18)`; Cargo:
  `1.98.0 (797e8a9bc 2026-08-05)`. No runtime or tool was installed or
  switched.
- Pre-edit `Cargo.lock` SHA-256:
  `c2fd2258a08d102069dfc2def0238970e66d550a35bb4550afb5a53d28b9ef86`.
- Pre-edit generated TypeScript SHA-256:
  `156c56374875090606dde45f6141b8f2a22a8eeeb3d1359423edf696893d3511`.
- `cargo build --release --workspace --locked` on the unchanged signed
  baseline exited 0.
- Newest release `rehow-core` rlib:
  `target/release/deps/librehow_core-0f407611f1e4f33d.rlib`, 2,890,726 bytes.
- `target/release/rehow-mcp`: 450,408 bytes.
- `target/release/rehow-desktop`: 9,937,192 bytes.

All six packet-prescribed pre-edit locked trees were captured before the
manifest edit in one private temporary evidence directory. The unrelated root
file named by `AGENTS.md` was excluded from all evidence and operations.

## Resolution and tree delta

The manifest received only the exact pins:

```toml
rowan = "=0.16.1"
saphyr-parser = "=0.0.12"
yaml-edit = { version = "=0.3.1", default-features = false }
```

Exactly one unlocked `cargo fetch` was run. It resolved eight packages and did
not compile or execute build scripts or procedural macros. The lock diff adds
only the identities in set A, adds the three intended `rehow-core` edges, and
disambiguates one pre-existing `rustc-hash` reference to `2.1.3`; no existing
identity, version, source, or checksum changed.

The matching six post-resolution locked trees and three inverse trees were
captured. Every mechanical `diff -u` exited 1 (expected differences); none
exited 2:

| comparison | exit |
| --- | ---: |
| Workspace normal | 1 |
| Workspace default features | 1 |
| core default features | 1 |
| core all features | 1 |
| Workspace all features | 1 |
| Workspace all features, Linux host target | 1 |

The semantic identity differences are the same dependency branch in all six
configurations. Remaining diff movement is Cargo tree display deduplication or
sibling ordering after the new branch became the first rendering of an
already-reachable identity. Feature labels must nevertheless be accounted for
per capture because the Workspace graph enables two pre-existing proc-macro
features that the core-only graph does not; the exact accounting appears
below.

The original baseline/post snapshots lived in the packet-required private
temporary directory and are no longer available, so this document does not
invent snapshot hashes. The signed baseline is `f39d7ab`, its lock identity is
the pre-edit hash above, and the current post-resolution manifest/lock delta is
the one classified below. An independent Security Reviewer reproduced the
six locked tree commands against those two states, reproduced the A/B/C
classification, and then reproduced the exact post-resolution feature labels
with non-compiling `cargo tree` commands. The packet's commands at lines
439--444 and 458--466 are therefore the durable reproduction recipe; all use
`--locked`, and the target-qualified capture uses
`x86_64-unknown-linux-gnu`.

### Set A — newly locked identities

Every source is `registry+https://github.com/rust-lang/crates.io-index`.

| identity | lock/archive SHA-256 checksum |
| --- | --- |
| arraydeque 0.5.1 | `7d902e3d592a523def97af8f317b08ce16b7ab854c1985a0c671e6f15cebc236` |
| countme 3.0.1 | `7704b5fdd17b18ae31c4c1da5a2e0305a2bf17b5249300a9ee9ed7b72114c636` |
| hashbrown 0.14.5 | `e5274423e17b7c9fc20b6e7e208532f9b19825d82dfd615708b70edd83df41f1` |
| rowan 0.16.1 | `417a3a9f582e349834051b8a10c8d71ca88da4211e4093528e36b9845f6b5f21` |
| rustc-hash 1.1.0 | `08d43f7aa6b08d49f382cde6a7982047c3426db949b1424bc4b7ec9ae12c6ce2` |
| saphyr-parser 0.0.12 | `a0b1b76852c5643f558e7347e4de549474c3ecb7154295a189f4668428c1b453` |
| text-size 1.1.1 | `f18aa187839b2bdb1ad2fa35ead8c4c2976b64e4363c386d45ac0f7ee85c9233` |
| yaml-edit 0.3.1 | `482be37565376de9a5502a56f19b68ecf0d4f85298ac89f3a62e03d7e4ecf9e3` |

The SHA-256 of every cached `.crate` archive exactly matched its new lock
checksum.

### Set B — already locked, newly reachable through the Saphyr edge

Every source is the same crates.io registry source.

| identity | lock/archive SHA-256 checksum |
| --- | --- |
| thiserror 2.0.20 | `ec86235f5fcc2a73650310756d2ac5b138a5780bbbdfae3eeccec992c435ba4f` |
| thiserror-impl 2.0.20 | `bc04cd3e1236dd4a98afca4569f2deb3f120e5422a4023be2cb683f8486292af` |
| proc-macro2 1.0.107 | `985e7ec9bb745e6ce6535b544d84d6cd6f7ad8bd711c398938ae983b91a766d9` |
| quote 1.0.47 | `1fbf4db142a473a8d80c26bbf18454ed458bf8d26c8219c331daecfdbd079001` |
| syn 3.0.5 | `12df2e0110f65b775f769bb17ef989067a1d931b2eb822bd4346631eeada89f9` |
| unicode-ident 1.0.24 | `e6e4313cd5fcd3dad5cafa179702e2b244f760991f45397d14d4ebf38247da75` |

These identities were already reachable from `rehow-core` through Phase 1B's
derive/TS dependencies. They now also occur under
`saphyr-parser -> thiserror`.

### Set C — already locked identities with changed enabled features

Set C is empty. Comparing each baseline/post pair within the same captured
configuration shows no newly enabled feature on an already-locked identity.
That statement does not mean every captured configuration has the same feature
set. The normal-edge capture (`cargo tree --workspace --locked -e normal`)
classifies package reachability and intentionally suppresses feature edges; its
corresponding feature account is the Workspace-default capture. The five
feature-edge captures resolve to these two exact profiles for every A union B
member:

| package | core default and core all | Workspace default, Workspace all, and Workspace all Linux host |
| --- | --- | --- |
| arraydeque 0.5.1 | none | none |
| countme 3.0.1 | `default` | `default` |
| hashbrown 0.14.5 | `inline-more` | `inline-more` |
| rowan 0.16.1 | `default` | `default` |
| rustc-hash 1.1.0 | `default`, `std` | `default`, `std` |
| saphyr-parser 0.0.12 | `default` | `default` |
| text-size 1.1.1 | `default` | `default` |
| yaml-edit 0.3.1 | none | none |
| thiserror 2.0.20 | `default`, `std` | `default`, `std` |
| thiserror-impl 2.0.20 | `default` | `default` |
| proc-macro2 1.0.107 | `default`, `proc-macro` | `default`, `proc-macro`, `span-locations` |
| quote 1.0.47 | `default`, `proc-macro` | `default`, `proc-macro` |
| syn 3.0.5 | `clone-impls`, `default`, `derive`, `parsing`, `printing`, `proc-macro` | `clone-impls`, `default`, `derive`, `extra-traits`, `full`, `parsing`, `printing`, `proc-macro` |
| unicode-ident 1.0.24 | `default` | `default` |

These labels were confirmed with the packet's five `cargo tree ... -e
features` configurations plus exact inverse queries (`-i name@version`); those
commands inspect resolution metadata and do not compile, test, build, run a
build script, or execute a procedural macro. The Workspace-only
`span-locations`, `full`, and `extra-traits` labels existed in the matching
baseline Workspace captures, so they do not enter C.

The exact precompile audit union is therefore the 14 identities in A union B.

## Local source review of A union B

The exact unpacked sources were inspected beneath Cargo's crates.io source
directory, and the corresponding cached archives were checksum-verified. A
crates.io package has no lockfile VCS revision; the exact reviewed revision is
the immutable `(name, version, registry source, archive checksum)` identity.
Repository URLs below are provenance metadata, not substitutes for that
identity.

| package | license | declared MSRV | enabled features | unsafe review | build/proc-macro behavior |
| --- | --- | --- | --- | --- | --- |
| arraydeque 0.5.1 | MIT/Apache-2.0 | undeclared | no feature label under Saphyr (`std` not enabled) | 72 code matches, all in its fixed-capacity deque implementation; reachable dependency code | no build script; not proc-macro |
| countme 3.0.1 | MIT OR Apache-2.0 | undeclared | `default` (empty); neither `enable` nor `print_at_exit` | one unsafe block exists only in `imp.rs` behind disabled `enable`; not feature-reachable | no build script; not proc-macro |
| hashbrown 0.14.5 | MIT OR Apache-2.0 | 1.63.0 | `inline-more` only | 288 code matches in core table/allocation/SIMD implementation plus disabled optional integrations; core unsafe is reachable | no build script; not proc-macro |
| rowan 0.16.1 | MIT OR Apache-2.0 | 1.77.0 | `default` (empty); `serde1` absent | 53 production code matches in owned arc, green tree, cursor, and linked-list internals; reachable | explicitly no build script; not proc-macro |
| rustc-hash 1.1.0 | Apache-2.0/MIT | undeclared | `default`, `std` | none | no build script; not proc-macro |
| saphyr-parser 0.0.12 | MIT OR Apache-2.0 | 1.85.0 | `default` (empty); `debug_prints` absent | none | explicitly no build script; not proc-macro |
| text-size 1.1.1 | MIT OR Apache-2.0 | undeclared | `default` (empty) | none | no build script; not proc-macro |
| yaml-edit 0.3.1 | Apache-2.0 | 1.70 | none; `default`/`binary` and `regex` absent | one reachable conversion in `src/nodes/mod.rs:174`, transmuting rowan's raw kind to yaml-edit's closed syntax kind | explicitly no build script; not proc-macro |
| thiserror 2.0.20 | MIT OR Apache-2.0 | 1.71 | `default`, `std` (unchanged) | none in library source | build script writes generated private glue and runs bounded rustc capability/version probes; already executed by signed Phase 1B, no newly enabled behavior |
| thiserror-impl 2.0.20 | MIT OR Apache-2.0 | 1.71 | `default` (empty, unchanged) | no unsafe code construct; one textual match in expression scanning | procedural macro; already reachable/executed by Phase 1B |
| proc-macro2 1.0.107 | MIT OR Apache-2.0 | 1.71 | core: `default`, `proc-macro`; Workspace: those plus `span-locations` (all unchanged within configuration) | six exact Rust occurrences, classified below; compiled API/internals are reachable, but no claim is made that every unsafe path is dynamically called | build script performs rustc version/capability probes; with Workspace `span-locations` it additionally emits the `span_locations` cfg and enables source-location tracking; already reachable/executed by Phase 1B, but the changed graph has not executed it; not itself proc-macro |
| quote 1.0.47 | MIT OR Apache-2.0 | 1.71 | `default`, `proc-macro` (unchanged) | none | build script checks rustc version for diagnostic cfg; already reachable/executed; not itself proc-macro |
| syn 3.0.5 | MIT OR Apache-2.0 | 1.71 | core set plus Workspace-only `full`, `extra-traits`, exactly as tabulated above (unchanged within configuration) | 40 code matches in token-buffer/cursor/thread and parser internals; reachable to the procedural macro | explicitly no build script; not proc-macro |
| unicode-ident 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | 1.71 | `default` (empty, unchanged) | two reachable unchecked table lookups after computed bounds | explicitly no build script; not proc-macro |

The complete `proc-macro2 1.0.107` Rust-source unsafe inventory is exactly six
occurrences:

| source occurrence | classification under captured features |
| --- | --- |
| `src/fallback.rs:107` | reachable fallback token-stream ownership path; reads a `ManuallyDrop` field with `ptr::read` before converting the reference-counted builder to owned storage |
| `src/fallback.rs:1002` | crate-private unsafe literal constructor compiled with `proc-macro`; it owns the supplied representation without validation and is reached only through an unsafe caller contract |
| `src/wrapper.rs:802` | crate-private unsafe dispatch constructor compiled with `proc-macro`; selects compiler or fallback literal construction according to proc-macro context |
| `src/wrapper.rs:806` | explicit unsafe call to the fallback constructor in that dispatch path |
| `src/lib.rs:1421` | doc-hidden public unsafe constructor intended for `quote!`; compiled and available to already-reachable procedural-macro dependencies |
| `src/lib.rs:1422` | explicit unsafe call from the public constructor into the implementation constructor |

The source has `#![deny(unsafe_op_in_unsafe_fn)]`, so the three unsafe calls are
explicit even inside unsafe functions. `span-locations` introduces no seventh
unsafe occurrence: its material security-relevant difference is build-script
behavior. In Workspace default/all/all-host captures, `build.rs` observes the
feature, emits `cargo:rustc-cfg=span_locations`, and still performs the
`proc-macro` rustc version/capability probes; core-only captures perform the
probes without that feature-driven cfg. This is an inventory and static
reachability classification, not an unsupported assertion that the dependency
is unsafe-free or that every compiled unsafe path executes in Rehow.

## Packaged VCS provenance

Each reviewed archive's `.cargo_vcs_info.json` supplied the following exact
upstream provenance. An empty `path_in_vcs` is recorded as `""`; the
rustc-hash archive omits that key rather than asserting an empty path.

| package | SHA-1 | path_in_vcs |
| --- | --- | --- |
| arraydeque 0.5.1 | `2ccf30574a2fac91311a0fca7b32f99d41f5680e` | `""` |
| countme 3.0.1 | `9e037c4293fb8ef6c1c632094349734955f06d89` | `""` |
| hashbrown 0.14.5 | `025d0f58b62d074f5fd660a22fe293cdc237d589` | `""` |
| rowan 0.16.1 | `e25dbdf9cb7f7757eb85c74a69c8a7d4a4fae67b` | `""` |
| rustc-hash 1.1.0 | `6fc9a706a9312ee801c7e77ca06c0782fa9eadef` | key absent |
| saphyr-parser 0.0.12 | `5c45acd365c711e3d92f0ed4a0ceabe349cde514` | `parser` |
| text-size 1.1.1 | `5f22fbe12550a2c70caff9a8c5347d6722370469` | `""` |
| yaml-edit 0.3.1 | `4cbd1d452f0bc5bbb0f92d558c30191180754c2a` | `""` |
| thiserror 2.0.20 | `b1d5db5e039275d95bf7536a2b2192aeb4dc28bf` | `""` |
| thiserror-impl 2.0.20 | `b1d5db5e039275d95bf7536a2b2192aeb4dc28bf` | `impl` |
| proc-macro2 1.0.107 | `ed8a5497669cd63db33bf24646f261b012bbbc4a` | `""` |
| quote 1.0.47 | `723dcb47d3f0ddc896e17287c8a8d3f2ea2317d5` | `""` |
| syn 3.0.5 | `e0ad92d68b588c964e82bb74c3bd3e1e99f97ceb` | `""` |
| unicode-ident 1.0.24 | `5b54a632702b5744a1c40ea01c127c0ac0498172` | `""` |

Repository metadata observed in the packaged manifests:

- arraydeque: `https://github.com/andylokandy/arraydeque`
- countme: `https://github.com/matklad/countme`
- hashbrown: `https://github.com/rust-lang/hashbrown`
- rowan: `https://github.com/rust-analyzer/rowan`
- rustc-hash: `https://github.com/rust-lang-nursery/rustc-hash`
- saphyr-parser: `https://github.com/saphyr-rs/saphyr`
- text-size: `https://github.com/rust-analyzer/text-size`
- yaml-edit: `https://github.com/jelmer/yaml-edit`
- thiserror/thiserror-impl: `https://github.com/dtolnay/thiserror`
- proc-macro2: `https://github.com/dtolnay/proc-macro2`
- quote: `https://github.com/dtolnay/quote`
- syn: `https://github.com/dtolnay/syn`
- unicode-ident: `https://github.com/dtolnay/unicode-ident`

All declared MSRVs are at or below the frozen Rust 1.98.0 baseline. The four
packages with no declared MSRV are recorded as such; no MSRV was inferred.

The official crates.io version endpoint was queried successfully on
2026-09-06 for all 14 exact identities. Every response reported `yanked:
false`, and every returned checksum, license, declared MSRV, and repository
matched the lock/archive/packaged-manifest evidence above.

Duplicate versions in the resolved Workspace include `hashbrown` 0.12.3,
0.14.5, and 0.17.1; `rustc-hash` 1.1.0 and 2.1.3; `thiserror`/
`thiserror-impl` 1.0.69 and 2.0.20; and `syn` 1.0.109, 2.0.119, and 3.0.5.
Other union
members have no second version in the lock. These are parallel locked
identities, not lock replacement.

## Current registry and advisory evidence

- Neither `cargo-audit` nor `osv-scanner` is installed. Per packet, neither
  was installed.
- An initial crates.io request without a descriptive user agent returned HTTP
  403. Retrying correctly with a descriptive user agent succeeded for all 14
  identities; the current yank audit is complete and clean as recorded above.
- The official OSV query endpoint could not be resolved from the shell
  (`Could not resolve host: api.osv.dev`). No per-package API response was
  obtained, so that failed attempt is not represented as a clean result.
- OSV's official documentation identifies its continuously exported public GCS
  bucket as the source for full and per-ecosystem data dumps. The Orchestrator
  downloaded the crates.io dump from
  `https://storage.googleapis.com/osv-vulnerabilities/crates.io/all.zip` and
  its `modified_id.csv` on 2026-09-06. The server reported `Last-Modified`
  `2026-09-06 16:01:50 UTC` for the zip and `16:01:44 UTC` for the CSV. The
  local SHA-256 values were respectively
  `329c9c57d34ff1cf75aa6f05f258b4412ff5238d9e3f697b24217dc54c4f1bcf`
  and `ac8a59d008e144b2057926bba9ac4c0db69dd046b02e7f7d7bc84a5bbb21935a`.
  The CSV's newest row was modified at `2026-09-06T15:55:53.855314026Z`.
  See the official [OSV data-source and dump documentation](https://google.github.io/osv.dev/data/).
- A bounded in-memory scan parsed all 2,811 JSON entries in that crates.io zip,
  selected every `affected.package` whose exact name was in A union B, and
  compared the reviewed version with each affected version/range. Thirteen
  package names had no OSV record. `hashbrown` had two equivalent records,
  `GHSA-wwq9-3cpr-mm53` and `RUSTSEC-2024-0402`, both affecting only
  `>=0.15.0,<0.15.1`; reviewed `hashbrown 0.14.5` is explicitly outside that
  range. No non-withdrawn OSV record affects any of the 14 exact union
  identities.
- The Orchestrator cloned the official RustSec advisory database read-only with
  Git hooks disabled into a private temporary directory. Remote `main` and the
  clone both resolved to
  `5a0ebedfe8bdd2e295b171f4162f8c977bcad9a5` at query time; the latest commit is
  dated `2026-09-02T11:13:32+02:00`. An exhaustive package-field search over
  the database found only `RUSTSEC-2024-0402` among the 14 union names. Its
  database record declares `unaffected = ["< 0.15.0"]` and
  `patched = [">= 0.15.1"]`, so `hashbrown 0.14.5` is unaffected. The other
  13 exact package names have no RustSec advisory. See the official
  [RustSec advisory database](https://github.com/RustSec/advisory-db).

The combined current crates.io, OSV dump, and RustSec database review finds no
yank or published advisory affecting an exact A/B/C-union identity. It does
not claim that the absent local scanner or failed OSV API request succeeded;
the documented official data exports supplied the exhaustive replacement
evidence. No changed-graph compilation, test, rustdoc, build, or parser-source
implementation preceded this audit.

## Commands and preserved boundary

Completed before the manifest edit: tool versions, `rustc -vV`, three signed
commit verifications, both baseline hashes, locked release build, exact release
artifact sizes, and all six prescribed baseline trees.

Completed after the edit: exactly one `cargo fetch`, manifest/lock diff,
matching six post trees, three inverse trees, six mechanical diffs, archive
checksum inspection, local manifest/source/unsafe/build/proc-macro inspection,
`cargo tree --workspace --locked -d`, availability checks for the two audit
tools, official crates.io queries, the failed OSV API attempt, the official OSV
crates.io dump scan, and the current official RustSec database scan.

After the clean security gate, the exact required first command
`cargo check -p rehow-core --locked` exited 0. Focused parser, trybuild,
formatting, Clippy, contract, rustdoc, full Workspace, release, and native
Tauri gates subsequently compiled and executed the audited graph and exited
0. The final six tree configurations and three inverse trees were repeated
with `--locked`; their identities/features match the audited post-resolution
graph, with `saphyr-parser/debug_prints` and `yaml-edit/binary` absent.

## Pre-Boundary-4 candidate identity and artifact measurements

This historical measurement was captured before the final Boundary 4
corrections. It is retained as candidate history and is superseded by the
Boundary 4 final closure rerun below.

- Candidate `Cargo.lock` SHA-256:
  `a0b7fd09e28f8676543d6d8fffd8664ca60b0979f51776a74a5f5e27311290d5`.
- Candidate generated TypeScript SHA-256:
  `156c56374875090606dde45f6141b8f2a22a8eeeb3d1359423edf696893d3511`,
  byte-identical to baseline after generation and contract checks.
- Pre-Boundary-4 candidate release `rehow-core` rlib:
  `target/release/deps/librehow_core-183140c95b2f8583.rlib`, 3,375,408 bytes
  (baseline 2,890,726; delta +484,682).
- Candidate `target/release/rehow-mcp`: 450,408 bytes (delta 0).
- Candidate `target/release/rehow-desktop`: 9,938,632 bytes (delta +1,440).
- `cargo doc -p rehow-core --no-deps --locked` exited 0; searches found no
  Phase 1C parser, artifact, failure, or test-control name in public rustdoc.
- Production parsing-source searches found no Rehow `unsafe` block and no
  forbidden dependency call from ADR 0013. The private dependency adapter
  fixture compiled the allowed yaml-edit/rowan and Saphyr calls.

## Boundary 4 final closure rerun — 2026-09-07

The complete Boundary 4 candidate gate was rerun after the final test-control,
corpus, property, precedence, meter, and dependency-audit corrections. Every
command exited 0. No dependency or feature changed, and frozen pnpm
materialization reported the repository already up to date.

- `Cargo.lock`: `a0b7fd09e28f8676543d6d8fffd8664ca60b0979f51776a74a5f5e27311290d5`.
- `crates/rehow-core/Cargo.toml`:
  `93d1895b679c25c7d510536e7c43eb54ff17b79dd08d029afe15e3b024b59749`.
- generated TypeScript:
  `156c56374875090606dde45f6141b8f2a22a8eeeb3d1359423edf696893d3511`;
  generation and contract checks left it byte-identical.
- final release core rlib:
  `target/release/deps/librehow_core-183140c95b2f8583.rlib`, 3,566,708 bytes
  (baseline delta +675,982 bytes).
- final `target/release/rehow-mcp`: 450,408 bytes (baseline delta 0).
- final `target/release/rehow-desktop`: 10,132,336 bytes (baseline delta
  +195,144 bytes). Tauri emitted only its pre-existing recommendation about
  the `.app` identifier suffix.

The six final locked graph captures have these SHA-256 values:

| Capture | SHA-256 |
| --- | --- |
| Workspace normal edges | `2115ea19967cded0684ea18eb897dde9b56ad72431863efb0b8f6b187a7c005b` |
| Workspace default features | `dc48b46f420fb3de48aad2579f0cbd0288813cb00102014e9489f58e8a06f824` |
| core default features | `5fe115b374905191f01c309a068379d9456eeaace4b5214623db8f1375beaa79` |
| core all features | `5fe115b374905191f01c309a068379d9456eeaace4b5214623db8f1375beaa79` |
| Workspace all features | `dc48b46f420fb3de48aad2579f0cbd0288813cb00102014e9489f58e8a06f824` |
| Workspace all features, Linux host | `765ba5e40ca0c1f21df3a75580d2f5071c6d1aa7929ef5a977aa32a7fb7ccf88` |

These equal the previously audited post-resolution captures. The three final
Workspace inverse captures resolve only the intended direct `rehow-core`
edges and transitive parents: Saphyr
`c0adf467ebf23b966971989c396d6e789e0e45f27a74cad6f82747bd79698f41`,
yaml-edit `6d7ddae96047a9d1b9c9156e516205b368af6f25918d79cabe309d929c7480e6`,
and rowan `92d9480d30152c26083b2c58979bd0288813cb00102014e9489f58e8a06f824`.
Neither `saphyr-parser/debug_prints` nor `yaml-edit/binary` occurs.

`production_dependency_imports_and_calls_match_the_frozen_allowlist` now
enumerates the exact dependency import lines and associated qualified calls,
and rejects unexpected direct dependency namespaces. It also freezes the
complete public structural/navigation/conversion method inventory derived from
rowan 0.16.1 `api.rs`, `cursor.rs`, `utility_types.rs`, and `ast.rs` for typed
and raw node/token/element types, child/preorder cursors, `NodeOrToken`,
offset/walk types, and AST pointers. Across `mod`, `source`, `presentation`,
`semantic`, `scalar`, and `reconcile`, the audit rejects both dot and
qualified/UFCS spellings of every forbidden method, rejects qualified forms of
dot-approved calls, and independently counts the generic `::parse`, `::cast`,
and `::new_from_str` suffixes against their exact permitted qualifiers. It pins
the two aggregate `::parse` occurrences as one internal `semantic::parse` and
one dependency `YamlFile::parse`; alias/`<Type>` additions and replacements are
negative probes. It pins
every imported dependency type, trait, and alias spelling. The scalar region
continues across inline test-meter `cfg` seams through the final production
helper rather than truncating at the first attribute. The companion tests compile
every allowed yaml-edit/rowan/Saphyr operation, reject the complete ADR
forbidden catalog, inspect every retained
presentation/semantic/artifact/deferred-draft record region for dependency
handles, require both exhaustive 50-kind matches, require the single exact
dead-code reason, and reject recursive traversal and pseudo-lexer helpers.

The frozen inventory source basis is the locally resolved rowan 0.16.1 crate:
`api.rs` SHA-256 `31c2ae43432bae7dff12d4d2d1a76c62c3f2d74a9d00828b82d38a01d61334c8`,
`cursor.rs` `0b191dd7b9946e8ed0e436b9af5d13e57911a755fec788d1d87f3aee365d7489`,
`utility_types.rs` `ce0874ae3e2b1eccb77025322c4453ceafef983f5d41b442448fb848259edf06`,
and `ast.rs` `f6e803e1cd1099a6d80a27c8b7fda965dc664bad6d541788699fe083169971e2`.

Normal rustdoc was regenerated. Exact-name searches found no parser entry,
artifact, failure/limit, YAML/file coordinate, comment, round-trip, or test
control symbol. The corresponding generated-TypeScript search also found none,
including every `UnknownYaml*` name. Production parser modules contain no
Rehow unsafe block and no `ParseTestControl`, `forced_issue`, or `TailReducer`.
The 24-case public API compile suite, including all six Phase 1C negatives and
the Phase 1B positive, passed.
