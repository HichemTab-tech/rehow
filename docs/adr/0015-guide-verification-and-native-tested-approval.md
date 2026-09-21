# ADR 0015: Guide Verification and Native Tested Approval

- Status: Accepted
- Date: 2026-09-18
- Owners: Architect and Security Reviewer
- Scope: Workspace schema v1 Guide trust

## Context

`tested` is a user trust statement, not ordinary metadata. ADR 0014 deliberately refused to calculate or accept a verification hash until its canonical content, transition authority, and mismatch behavior were frozen.

## Decision

### Verification hash v1

The core alone calculates `verify:v1:<lowercase-hex>` with BLAKE3. The input is:

1. ASCII `rehow-guide-verification-v1`, then one NUL byte.
2. Unsigned 64-bit big-endian field count `10`.
3. In this exact order: `body`, `title`, `summary`, `applies_to`, `prerequisites`, `warnings`, `files_affected`, `related_guide_ids`, `references`, `attachments`.
4. Each field is `frame(name) || presence`. Presence is one byte, `0x00` when an optional field is absent or `0x01` when present. A present field then appends `frame(value)`. `frame(x)` is an unsigned 64-bit big-endian byte length followed by `x`.
5. `body` is always present and its value is the exact Markdown body bytes. Other values use the canonical binary YAML value encoding already frozen in `docs/content-model.md`. Known nested extension members therefore participate automatically.
6. Append the count of unknown root entries as unsigned 64-bit big-endian. Sort those entries by exact UTF-8 key bytes, then append `frame(key) || frame(canonical-value)` for each.

The fixed known-field set deliberately excludes schema/identity, Category, Tags, aliases, status, creation/update/verification/archive fields, and filesystem path. Those are organizational or trust bookkeeping, not instructions the user tried. Root extensions and extensions nested inside included known fields are verification-relevant so a future or external instruction-bearing extension cannot retain trust accidentally.

Canonical value encoding preserves negative floating zero for values. Mapping keys normalize both signs of zero for equality, and mapping pairs sort by canonical key encoding then canonical value encoding. Hash construction is bounded by the parser's existing source, node, depth, and collection limits and uses fallible allocations. Cross-language fixed vectors are mandatory tests and are recorded beside the implementation before this phase is accepted.

Fixed vector: title `Install tool`, summary `Install the downloaded tool`, all optional verification fields absent, no unknown root entries, and exact body bytes `## Steps\n\nRun it.\n` produce `verify:v1:0966e3140e5e3a51d09c341a6322cf751ec7be076021cbd724bd2362b84dcd54`.

`scripts/verify-guide-hash-vectors.mjs` independently implements the framing and canonical-value encoding in JavaScript and hashes with pinned `@noble/hashes`. It checks the minimal vector plus fixed complex vectors for mappings, signed zero, nested known extensions, and sorted unknown roots. The Rust suite asserts the same digests. `pnpm test` runs this cross-language compatibility gate.

`verify:v1` applies only to Workspace/Guide schema v1. A future schema that changes verification meaning requires an ADR and a new verification-hash version. Rehow never silently upgrades a verification declaration. An unrecognized or mismatched hash makes a declared `tested` Guide effectively `stale` and produces a content-free Workspace diagnostic; it does not rewrite the file.

### Authority and transition

Only the core's native-confirming entry point may request approval. It takes a real parent window, canonical Guide ID, and exact expected revision; looks up the current Guide through the retained Workspace capability; and itself displays the native confirmation containing a control/bidi-safe title and canonical ID before entering the private mutation operation. There is no public headless transition function or constructible approval token. The renderer never supplies status, timestamp, hash, path, or an approval Boolean. Cancellation is a no-op. MCP can reuse this entry point only when it can supply the trusted native helper's real parent window, so calling it still cannot bypass the dialog.

The core reparses the current revision under the writer lock, rejects archived or malformed Guides, calculates the hash from those exact bytes, and atomically changes `draft` or `stale` to `tested`; it sets `last_verified_at` and `updated_at` to the trusted native timestamp and writes the calculated hash. A currently effective-tested Guide is a no-op. A declared-tested Guide whose hash mismatches may be approved again, replacing its verification pair. Round-trip/comment ambiguity refuses the whole operation.

Approval readiness uses the same bounded CommonMark/GFM parser as rendering. Reserved sections are exact, case-sensitive level-two heading labels after concatenating their rendered text: `Steps`, `Verification`, `Rollback`, and `Troubleshooting`. Inline formatting in a heading does not create a different label. Every reserved label may occur at most once; any duplicate is ambiguous and refuses approval. `Steps` must occur exactly once and contain non-whitespace text or code before the next level-one/two heading or end of body. The other reserved sections are recommended but not required. Rehow does not reorder or synthesize sections.

AI, MCP client assertions, import, adoption, and generic mutation APIs cannot invoke this authority. ADR 0044 permits the trusted standalone MCP adapter to request the same exact-revision transition only after its sealed native helper records explicit human approval; the client still cannot submit Tested state, consent, hash, time, path, or effects.

### Effective status

Storage calculates effective status while binding exact bytes. Declared draft, stale, and archived retain their lifecycle status. Declared tested is exposed as Tested only when its stored v1 hash equals the current calculated v1 hash; otherwise it is exposed to search/UI as Stale and a diagnostic is recorded. The declared value remains available internally for safe mutation planning.

## Consequences

- A user can trust Tested as evidence that the exact instruction-bearing content was approved.
- Organizational changes may preserve native trust in a later separately sealed operation; existing generic edits continue to stale Tested Guides.
- Hash code is format-sensitive by design and requires permanent vectors and migration discipline.
- Native confirmation is required for every transition/reapproval; there is no headless shortcut.
