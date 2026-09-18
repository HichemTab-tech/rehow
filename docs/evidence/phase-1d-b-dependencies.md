# Phase 1D-B dependency evidence

Date: 2026-09-18 (Africa/Algiers)

Status: **precompile audit passed**. The audit below completed before the
`rehow-core` manifest or lockfile was changed. No Node, pnpm, Rust toolchain, or
system package was installed or switched.

## Exact identities

| package | checksum | license | MSRV | yanked | advisory result |
| --- | --- | --- | --- | --- | --- |
| `focaccia 2.0.0` | `9ef16690750b6e9a238378f57d3c1325d42e2d82e29b7dfd75f6d387d753b050` | MIT AND Unicode-3.0 | 1.83.0 | no | OSV exact query: no vulnerabilities |
| `unicode-normalization 0.1.25` | `5fd4f6878c9cb28d874b009da9e8d183b5abc80117c40bbd187a1fde336be6e8` | MIT OR Apache-2.0 | 1.36 | no | OSV exact query: no vulnerabilities |
| `url 2.5.8` | `ff67a8a4397373c3ef660812acab3268222035010ab8680ec4215f38ba3d0eed` | MIT OR Apache-2.0 | 1.63 | no | OSV exact query: no vulnerabilities |

The checksums, yank state, licenses, and declared Rust versions were read from
the current crates.io API using an identifying user agent. Exact-version OSV
crates.io API queries returned empty result objects on the audit date. `url
2.5.8` was already locked transitively with the same checksum; the other two
identities were downloaded for source inspection by `cargo info`, which does
not compile or execute them.

## Source and graph review

- `focaccia 2.0.0` has no normal dependencies, build script, procedural macro,
  or unsafe code; it forbids unsafe code. Its packaged `CaseFolding.txt` and
  generated tables are Unicode 16.0.0, matching ADR 0014. The intended API is
  default full, non-Turkic `CaseFold` comparison.
- `unicode-normalization 0.1.25` has no build script or procedural macro. Its
  only normal edge is `tinyvec` with `alloc`; its tables report Unicode 17.0.0.
  The source has four narrowly allowed unsafe Hangul composition/decomposition
  regions using range-proved unchecked scalar construction. NFC is used only
  to compare one already bounded Attachment filename with its normalized form.
- `url 2.5.8` has no build script or procedural macro. It was already present
  in the lock graph through the desktop stack. One reachable unsafe block
  converts a locally generated ASCII decimal buffer to `str`; its digits are
  written before conversion. Rehow prevalidates original URL bytes and uses the
  parser as a second structural check, never as opener or authority.
- The proposed direct edges disable default features. `focaccia` adds no
  transitive identity; `unicode-normalization` reaches already-audited
  `tinyvec`; `url` uses its existing locked identity and transitive graph.
- None of the packages performs filesystem, process, network, logging,
  credential, or code-generation behavior at runtime for Rehow's binding use.

## Post-resolution verification

One non-compiling `cargo fetch` resolved the lock change. The lock adds exactly
`focaccia 2.0.0` and `unicode-normalization 0.1.25`; it adds direct
`rehow-core` edges for those packages and the already-locked `url 2.5.8`.
No existing identity, version, source, or checksum changed. `focaccia` has no
feature/dependency edge. `unicode-normalization` has no enabled crate feature
and reaches already-locked `tinyvec 1.13.2` with `alloc`, its default feature,
and `tinyvec_macros`; `url` reaches its already-locked alloc/IDNA graph. The
first changed-graph compilation was `cargo check -p rehow-core --locked`; it
exited 0.
