# Workspace and Guide Content Model

Status: schema v1 baseline; Phase 1D-A accepted and 1D-B/1D-C proposed in ADR 0014
Last updated: 2026-09-07

## Principles

- Files are understandable and editable without Rehow.
- Stable IDs, not paths/titles, define identity and references.
- Active content is friendly to Git diffs and merge tools.
- Schema versions are explicit. A newer major version is never written by an older app.
- Unknown valid mapping keys are retained through ordinary edits.
- Paths are relative, normalized UTF-8, and confined; managed content never follows symlinks.
- Timestamps are RFC 3339 UTC instants written with `Z`; UI display uses locale-aware formatting.
- Validated domain strings reject Unicode noncharacters (`U+FDD0..U+FDEF` and every scalar ending in `FFFE`/`FFFF`). U+FEFF is valid text but deterministic YAML output escapes it so its UTF-8 bytes never resemble a mid-document BOM.

## Version 1 directory structure

```text
my-rehow-workspace/
├── rehow.yaml
├── categories/
│   ├── linux-system--b6ba3a5551/
│   │   ├── category.yaml
│   │   ├── install-tarball--36c5735e10.md
│   │   └── create-desktop-launcher--08b7bed660.md
│   └── archived--9d704a23b8/
│       └── category.yaml
├── attachments/
│   └── 0199a1d7-7b0d-7d8e-a38d-f936c5735e10/
│       └── example.desktop
└── .rehow/
    ├── trash/             # recoverable removals
    └── backups/           # migration snapshots
```

The short ID suffix in human paths is the final 10 hexadecimal UUID characters (40 random bits for UUIDv7). It is a collision-resistant hint only; the full in-file ID is authoritative. Path components use lowercase ASCII kebab slugs, capped at 80 bytes before the suffix. Collisions add a deterministic numeric suffix. Editing a Category name or Guide title changes metadata only. An explicit `tidy path` operation may rename the human path without changing identity.

Runtime locks, recovery journals, and before-images live outside the Workspace under `$XDG_RUNTIME_DIR`/`$XDG_STATE_HOME`; current search/index records are disposable process-private anonymous temporary files and never live in the Workspace. `.rehow/` contains only recoverable trash and portable migration/bulk-operation backups. Rehow never edits an existing `.gitignore` silently; initialization of a new empty Workspace offers a generated ignore file as part of the explicit plan.

Category directories are exactly one level below `categories/`. A Category descriptor is exactly `<category-dir>/category.yaml`. Active/archived Guides are direct `.md` children of that directory; nested Guide directories are invalid in schema v1. The containing descriptor's `id` must equal the Guide's `category_id`; mismatch isolates the Guide with `INVALID_GUIDE`. `change category` is one journaled operation that moves the file and updates `category_id`. There is no ambiguous generic Guide move. `tidy path` changes only the filename; title edit changes only frontmatter.

## Workspace manifest

`rehow.yaml`:

```yaml
schema_version: 1
workspace_id: 0199a19f-4d15-7ca7-a645-e75311caa710
name: Hichem's Rehow
created_at: "2026-09-05T10:00:00Z"
updated_at: "2026-09-05T10:00:00Z"
settings:
  default_category_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551
```

Required keys: `schema_version`, `workspace_id`, `name`, `created_at`, `updated_at`. `schema_version` is a semantic integer in `1..=4294967295` (the current value is `1`); zero, negative, non-integer, or larger values are malformed rather than a compatibility class. `workspace_id` is a lowercase canonical UUIDv7; `name` is trimmed text of 1–200 Unicode scalar values; timestamps are RFC 3339 UTC instants written with `Z`, and `updated_at` must not precede `created_at`. `settings` is optional; its known optional `default_category_id` is a Category UUID. `null` is not a substitute for an omitted optional field. Paths are fixed by schema v1 rather than configurable, reducing traversal and interoperability ambiguity. Unknown top-level/settings keys are preserved.

Compatibility:

- `schema_version == 1`: current and writable after validation.
- supported older positive integer: inspect as `older_supported`; open read-only until the user accepts a migration plan and verified backup. The v1 registry is empty because schema v1 is the first released format; adding an entry requires an exact migration ADR and fixtures.
- greater than the application's maximum: `newer_unsupported`, read-only inspection only.
- missing, duplicate, non-integer, zero, negative, greater than `u32::MAX`, or structurally invalid version: malformed.

The parser rejects YAML aliases/anchors, duplicate mapping keys, custom tags, and resource-exhausting nesting/size. Numeric limits are defined below and return errors rather than truncating canonical content.

## Category descriptor

`categories/<slug>--<short-id>/category.yaml`:

```yaml
schema_version: 1
id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551
name: Linux System
summary: Host configuration, boot, devices, and desktop integration.
order: 1000
status: active
created_at: "2026-09-05T10:01:00Z"
updated_at: "2026-09-05T10:01:00Z"
```

Required Category keys are `schema_version`, `id`, `name`, `order`, `status`, `created_at`, and `updated_at`. `summary` is optional. `schema_version` is semantic integer `1`; `id` is a lowercase canonical UUIDv7; `name` is trimmed text of 1–200 Unicode scalar values; `summary`, when present, is trimmed text of 1–500 Unicode scalar values; `order` is a semantic YAML integer in the complete signed 64-bit range; `status` is `active` or `archived`; timestamps are RFC 3339 UTC instants written with `Z`, and `updated_at` must not precede `created_at`. `null` is not an omitted optional field. Unknown top-level string keys are extensions and are retained within common YAML limits; invalid reserved keys are errors, not extensions.

Portable `category.yaml` stores `order` as a YAML integer. Category JSON request/response DTOs use `SignedInt64Decimal`, a quoted canonical decimal string, because JavaScript numbers cannot represent the complete range. DTOs reject JSON numbers, `+1`, `01`, `-0`, whitespace, and overflow. Domain/YAML and protocol DTOs are separate mapped types; new YAML writes use plain canonical base 10, while an existing valid spelling remains byte-identical until that field is edited.

Reordering may renumber all Category sort keys in one journaled action; it requires a Workspace inventory token. `settings.default_category_id`, when present, must reference an active Category. Archiving/removing that Category must atomically replace or clear the default under the expected manifest revision; otherwise the plan is refused. Removing a Category requires no active/archived Guides and moves the descriptor/directory to trash. Nonempty removal returns a plan with blocking Guide IDs; it never cascades silently.

## Guide Markdown

Guide files are UTF-8 Markdown with YAML frontmatter delimited by exactly `---` lines, then CommonMark/GFM body content. Rehow writes one blank line after the closing `---`. Since ADR 0063, that blank line is optional on read: the body may start on the next line, or the file may end at the delimiter, and edits keep whatever layout the file has.

A Guide filename is any safe filename ending in `.md` (single NFC component of at most 255 bytes; no `/`, `\`, NUL, controls, bidi controls, BOM, or noncharacters; no leading `.`; no trailing `~`). Rehow-created files keep the `<slug>--<id-suffix>.md` form, but hand-written names such as `My Notes.md` are first-class. Other entries are reported with a percent-escaped inert display path.

### Hand-written (unadopted) Guides

A Guide file that lacks complete frontmatter is listed as **unadopted** instead of malformed when it has:

- no opening `---` line;
- an opening `---` with no closing `---` line, or a block that is not a YAML mapping (the whole file minus a BOM is the body);
- a block holding only blank lines and comments; or
- a valid mapping that lacks required fields. Every present field is validated exactly as for a managed Guide.

Missing metadata is derived and never written during listing:

- `title`: the first level-one ATX heading outside fenced code, else the humanized filename. It is capped at 200 scalars, with invisible and spoofing scalars removed.
- `summary`: the first paragraph, collapsed to one line and capped at 500 scalars. It may be empty.
- `status`: `draft`. The effective status is always `draft`.
- `tags`/`aliases`: empty.
- Timestamps: the file mtime.
- `id`: the file's valid `id` when present. Otherwise it is BLAKE3(`rehow-derived-guide-v1\0` ‖ workspace id ‖ relative path), truncated to a UUIDv7 shape with a zero 48-bit timestamp. Renaming the file changes this id.

An invalid reserved field (for example `status: tested` without verification fields), invalid YAML, or invalid UTF-8 is still a malformed Guide. Its issue carries a content-free diagnostic: a code, an optional known field, and an optional line/column.

The first explicit save **adopts** the file in one journaled, revision-checked write, without renaming it:

- With no usable frontmatter, Rehow keeps a leading BOM and adds canonical frontmatter plus one blank line before the unchanged body bytes. CRLF and trailing blank lines are preserved.
- With partial or comment-only frontmatter, every existing byte is kept, and only the missing required keys are inserted before the closing `---`, using the frontmatter line ending. Flow-style or indented root mappings are refused.

The derived id is never written. The file's own `id` or a fresh UUIDv7 is used and returned to the caller. `created_at` is `min(mtime, now)`, `updated_at` is `now`, and an empty summary becomes the title. An unadopted Guide cannot be approved as `tested`.

````markdown
---
schema_version: 1
id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10
title: Install a Downloaded Tarball
summary: Extract a release archive under ~/Programs and add a stable launcher.
category_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551
tags:
  - linux
  - installation
aliases:
  - install tar.gz
  - manual app install
status: tested
created_at: "2026-09-05T10:03:00Z"
updated_at: "2026-09-05T10:20:00Z"
last_verified_at: "2026-09-05T10:20:00Z"
verified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
applies_to:
  - platform: Arch Linux
    versions: rolling
prerequisites:
  - The archive has a trusted source and checksum.
warnings:
  - Inspect archive paths before extraction.
files_affected:
  - ~/Programs/example
related_guide_ids:
  - 0199a1ef-f149-718a-b2a7-2208b7bed660
references:
  - title: Upstream installation notes
    url: https://example.invalid/install
attachments:
  - id: 0199a264-ef5f-73b4-a238-a71ba1509898
    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.desktop
    title: Example desktop entry
    media_type: application/x-desktop
    sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
---

## Steps

1. Inspect the archive before extracting it.

   ```bash
   tar --list --file example.tar.gz
   ```

2. Extract it into `~/Programs`.

## Verification

Confirm the application reports the expected version.

## Rollback

Remove only the directory created by these steps.

## Troubleshooting

### “Permission denied” when launching

Check the executable bit and mount options.
````

### Required frontmatter

- `schema_version`: integer `1`.
- `id`: lowercase canonical UUIDv7.
- `title`: trimmed text, 1–200 Unicode scalar values.
- `summary`: trimmed text, 1–500 Unicode scalar values.
- `category_id`: existing Category UUID.
- `tags`, `aliases`: arrays of unique display strings after Unicode case-fold comparison.
- `status`: `draft | tested | stale | archived`.
- `created_at`, `updated_at`: RFC 3339 UTC instants written with `Z`; `updated_at` must not precede `created_at`.

### Optional frontmatter

- `last_verified_at`, `verified_content_hash`: written by the core together when a Guide is explicitly marked tested. `last_verified_at` cannot be later than the same operation's `updated_at`. They remain on `stale` to show what was last verified.
- `archived_from_status`, `archived_at`: written together when status becomes `archived`, enabling an exact restore to `draft`, `tested`, or `stale`.
- `applies_to`: objects with required `platform`, optional free-text `versions`/`notes`.
- `prerequisites`, `warnings`, `files_affected`: ordered strings.
- `related_guide_ids`: unique Guide UUIDs; missing targets are warnings, not parser failure.
- `references`: `{title, url}` where URL scheme passes policy.
- `attachments`: `{id, path, title, media_type?, sha256, size_bytes?}`. Path must match this Guide's managed Attachment directory.

Unknown keys are retained as concrete-syntax nodes within depth/size/type limits. Reserved keys with invalid values are errors, not treated as unknown.

ADR 0014 is normative for Phase 1D-A Manifest/Category and Phase 1D-B Guide/nested-value read-only binding. Its mutation/emitter sections remain proposed for Phase 1D-C. This document remains the human-readable schema overview; accepted ADR text controls where it is more specific.

### Unknown YAML semantic values

Rehow resolves accepted YAML with the YAML 1.2 core schema into the following parser-neutral internal shapes. The public domain types are opaque structs re-exported by `domain`; their representation enums and every field live in the private `domain::yaml_value` module, so sibling modules cannot construct an unchecked variant:

```rust
pub struct UnknownMetadataEntry { key: String, value: UnknownYamlValue }
pub struct UnknownMetadataDocument { entries: Vec<UnknownMetadataEntry> }
pub struct UnknownYamlValue(UnknownYamlValueRepr);
enum UnknownYamlValueRepr {
    Scalar(UnknownYamlScalar),
    Sequence(Vec<UnknownYamlValue>),
    Mapping(Vec<UnknownYamlMappingEntry>),
}
pub struct UnknownYamlMappingEntry {
    key: Box<UnknownYamlValue>,
    value: Box<UnknownYamlValue>,
}
pub struct UnknownYamlScalar(UnknownYamlScalarRepr);
enum UnknownYamlScalarRepr {
    Null,
    Bool(bool),
    Integer(CanonicalYamlInteger),
    Float(CanonicalYamlFloat),
    String(String),
}
pub struct CanonicalYamlInteger { negative: bool, digits: String }
pub struct CanonicalYamlFloat(CanonicalYamlFloatRepr);
enum CanonicalYamlFloatRepr {
    Finite { negative: bool, coefficient_digits: String, exponent10: CanonicalYamlInteger },
    PositiveInfinity,
    NegativeInfinity,
    NaN,
}
```

`CanonicalYamlInteger` and `CanonicalYamlFloat` expose public validated associated constructors and read-only accessors. Integer constructors require nonempty ASCII digits with no leading zero, normalize zero to positive, and reject every noncanonical state. Finite-float constructors require a normalized nonempty coefficient with no leading/trailing zero unless it is exactly `0`; a zero coefficient requires a zero exponent but may retain a negative float sign.

`UnknownMetadataDocument`, `UnknownMetadataEntry`, `UnknownYamlValue`, `UnknownYamlMappingEntry`, and `UnknownYamlScalar` expose read-only accessors only. They have no public associated constructor, public field, representation enum, `Default`, `From`/`TryFrom`, `Deserialize`, or mutable collection accessor. The only public assembly surface is an opaque `UnknownYamlDocumentBuilder` that accepts a checked, iterative semantic-event sequence (begin/end sequence or mapping, null, Boolean, canonical integer/float, and owned string), enforces mapping key/value state and a string-only top-level key, and returns values only from `finish_document`. Finishing requires exactly one complete mapping root and no partial frame; any error consumes/poisons the builder and returns no partial node or document. The exact method names/event wrapper are frozen by the Phase 1B packet, but no event or builder method exposes a representation enum or returns an unfinished `UnknownYamlValue`.

The builder enforces depth/node/entry/scalar limits, the 80 KiB aggregate stored-canonical-numeric-digit limit, semantic-key encoding/comparison work limits, and semantic duplicate rejection. It charges the ASCII byte length of every retained integer digit string and every finite-float coefficient and exponent digit string, with checked arithmetic, before accepting the scalar. Phase 1B tests the aggregate numeric-digit exact limit and one-over using directly constructed canonical values; Phase 1C separately meters source-to-canonical numeric conversion work. Clone/read-only views cannot create an invalid state. External-module compile-fail/API tests prove that callers cannot construct a scalar/value/entry/document directly, access a private representation or mutable field, deserialize one, or extract a partial value from a failed builder.

An integer removes sign/radix markers and leading zeroes into decimal `digits`; zero is exactly `{ negative: false, digits: "0" }`, so every spelling of negative zero normalizes to integer zero. A finite float concatenates decimal mantissa digits, subtracts fractional width from the explicit exponent, removes leading zeroes, then removes trailing zeroes while increasing `exponent10`. A nonzero coefficient has no leading/trailing zero; any positive zero is `{ negative: false, coefficient_digits: "0", exponent10: { negative: false, digits: "0" } }`, while negative zero retains `negative: true`. Exponents use the same arbitrary-precision canonical integer form. Timestamp-like untagged scalars are strings. Accepted spellings and implicit resolution are exactly the YAML 1.2.2 core-schema regular expressions; YAML 1.1 numeric separators and legacy booleans are strings unless an invalid explicit core tag attempts to force them.

Semantic key equality is tag/type-aware: alternate spellings within one scalar type compare by canonical value, integer and float remain distinct, positive and negative floating zero compare equal for duplicate detection, and all NaN spellings compare equal. Sequence equality is order-sensitive. Mapping equality is recursive and entry-order-insensitive even though encounter order is retained for deterministic traversal. Top-level extension names must decode as strings; nested extension mappings retain complex keys. Every mapping rejects duplicates under this equality before construction.

This semantic tree has no `Serialize`, `Deserialize`, TypeScript, parser, CST, comment, whitespace, scalar-style, BOM, line-ending, or byte-offset commitment in Phase 1B. It does not satisfy concrete-syntax preservation by itself and is never used to rewrite canonical content. Existing source bytes plus the reconciled presentation/span artifact defined by ADR 0011 remain authoritative for later edits.

Timestamp ownership is per document: the core supplies `created_at` and updates that document's `updated_at` only when it changes. Child edits do not rewrite the Workspace manifest timestamp. Import may retain valid source timestamps as provenance only; newly managed timestamps are core-generated. `archived` requires `archived_from_status` and `archived_at`; every other status forbids both. A stored `tested` declaration requires a verification pair, `stale` may retain one, and `draft` has neither. Binding never proves tested trust: until Phase 3 recomputes the accepted content hash, trust consumers treat the declaration as unverified/effectively stale without silently rewriting source.

### Body sections

Phase 1D treats the entire body as opaque bytes and does not enforce headings or readiness. ADR 0015 requires one nonempty exact level-two `Steps` section before native tested approval; `Verification`, `Rollback`, and `Troubleshooting` are strongly recommended. Duplicate exact reserved level-two headings refuse approval as ambiguous. Other headings/sections remain preserved.

Commands are fenced code blocks, optionally with a language such as `bash`, `sh`, `fish`, `ini`, or `text`. They are indexed and copyable but never executable. Error messages are indexed from headings/paragraphs under Troubleshooting plus quoted/inline/fenced content recognized by deterministic extraction.

## Status transitions

| From | To | Rule |
| --- | --- | --- |
| draft | tested | Native user approval only; readiness validation passes; core sets `last_verified_at` and `verified_content_hash`. |
| draft | stale | Allowed when known outdated. |
| tested | stale | Explicit action or compatibility signal accepted by user; retain last verification. |
| stale | tested | Native user approval after trying the Guide; core updates verification time/hash. |
| any active | archived | Explicit reversible action; write prior state to `archived_from_status` and set `archived_at`. |
| archived | prior status | Restore from `archived_from_status` if Category/filename constraints remain valid; clear archive fields. |

All create/import/adoption paths produce `draft`, including MCP; creation cannot set `tested`. A separate Phase 3 native approval operation is the only route to effective tested trust. Any change to body, title, summary, applicability, prerequisites, warnings, files affected, related Guides, references, or Attachment metadata automatically changes a tested declaration to `stale` while retaining its prior verification fields. Unknown metadata has no schema-v1 mutation API. Category, Tags, aliases, path tidying, and archive/restore are organizational only for a future trusted native origin; automation/MCP edits stale a tested declaration even for those fields. ADR 0014 defines the pre-Phase-3 declared-status rules.

`verified_content_hash` is `verify:v1:<hex>`, BLAKE3 over a domain separator plus length-framed exact body bytes and canonical encodings of all verification-relevant known/unknown frontmatter fields; organization/status/timestamp/verification fields are excluded. It never encodes unknown YAML through JSON.

The unknown-value canonical binary encoding is type tagged and recursively length framed with unsigned 64-bit big-endian lengths/counts: null `0x00`; Boolean `0x01` plus `0x00|0x01`; integer `0x02` plus sign byte `0x00|0x01` and framed ASCII canonical digits; float `0x03` plus subtype `0x00` finite, `0x01` positive infinity, `0x02` negative infinity, or `0x03` NaN, where a finite payload is sign byte, framed coefficient digits, exponent sign byte, and framed exponent digits; string `0x04` plus framed exact UTF-8; sequence `0x05` plus count and each framed child encoding in order; mapping `0x06` plus count and each framed key then framed value after pairs are sorted lexicographically by canonical key encoding and then value encoding. Mapping-key encoding recursively canonicalizes both signs of floating zero to positive zero because they are equal for duplicate-key purposes; ordinary value encoding retains the represented negative-zero sign. NaN has one encoding. Top-level unknown metadata entries sort by exact UTF-8 key bytes and encode as a count followed by framed key and framed value. This makes mapping encounter order presentation-only without narrowing arbitrary integers or complex keys.

ADR 0015 freezes the ordered known-field set, complete outer framing, native-only approval authority, fixed-vector requirement, effective-status behavior, and migration policy for `verify:v1`. If an externally edited file says `tested` but the accepted versioned hash does not match, the core reports effective status `stale` and a diagnostic without silently rewriting the file.

## Revisions and mutation

A document/Attachment revision is `rev:v1:<hex>`. The digest is BLAKE3 of ASCII `rehow-document-revision-v1`, one NUL byte, then `frame(normalized_relative_path_utf8)` and `frame(exact_file_bytes)`. `frame(x)` is an unsigned 64-bit big-endian byte length followed by `x`. Managed relative paths use `/`, ASCII schema directory slugs, no `.`/`..`, and NFC UTF-8 Guide and Attachment filenames (one shared filename-safety predicate; ADR 0063). Fixed vectors make this language-independent. Including the path means an external rename invalidates an open draft.

Fixed vector: relative path `rehow.yaml` and exact bytes `abc` produce `rev:v1:ec918848dad4d31267309c57c165f16c9d725f0d6eebfae93ec1d60212eea0c2`.

A full inventory is `snapshot:v1:<hex>`. It covers the manifest and every regular-file candidate in the fixed Category/Guide/Attachment roots, including archived, malformed, duplicate-ID, and orphan candidates. Valid records use their stable ID; otherwise `stable_id` is empty and the confined relative path identifies the diagnostic record. Each record is `(kind, stable_id, normalized_relative_path, rev:v1)`; records sort lexicographically by the UTF-8 bytes of those fields and are encoded as a domain separator, count, and length-framed fields. Unsafe symlink/special/hardlink candidates disable broad mutation and appear in separate bounded diagnostics because they cannot be safely hashed as canonical content. Trash, backups, XDG state, derived cache, and Git metadata are excluded. Operation plans additionally bind a capability identity plus the exact affected record set; Git uses a separate token covering HEAD, index, managed/unrelated worktree facts, and operation scope.

- Update/rename/move/status/archive/delete require the current document revision.
- Category reorder and broad operations require a current full inventory or narrower operation-specific effect-set token as documented by that operation.
- Create requires an expected absence marker and collision check.
- Metadata and body are parsed from the current bytes immediately before applying a patch.
- A body save whose proposed bytes exactly equal the current body is a no-op: it preserves bytes, revision, timestamps, and tested status. Actual body changes preserve every unaffected frontmatter byte but must also update `updated_at` and, when applicable, change `tested` to `stale`; if any mandatory companion span is unsafe, the entire save is refused. Metadata edits use reconciled concrete-syntax byte ranges: untouched known/unknown nodes, comments, key order, scalar quoting, BOM, line-ending style, and final-newline presence remain byte-identical; only authorized replacement, insertion, and full-entry-removal ranges may change. Adapters provide typed values, never YAML fragments; deterministic replacement bytes come only from the fixed-schema emitter in ADR 0011. If comments cannot be retained or either parser cannot prove a safe edit, mutation fails with `UNSAFE_ROUND_TRIP` and leaves every byte unchanged. Directives, anchors, aliases, non-allowlisted explicit tags, merge keys, and duplicate keys are invalid at read time. Golden aftermath fixtures cover comments, ordering, quoting, Unicode, BOM, CRLF/LF, missing final newline, and unsupported edits.

## Attachments

- Rehow copies regular files into `attachments/<guide-id>/`; it never stores arbitrary external absolute references.
- Source selection may be outside the Workspace only through a native-owned dialog that returns an opaque source capability. The source is opened read-only, bounded, and copied; its original path is not persisted by default.
- Symlinks, devices, FIFOs, sockets, directories, and any existing managed file with link count other than 1 are rejected. Import always creates a fresh destination inode with mode `0600`; executable source mode is never copied.
- Destination names are normalized display names plus collision-safe suffixes. Metadata path is authoritative and confined.
- SHA-256 verifies bytes and supports duplicate detection; it does not assert trust.
- Removal is a journaled move to trash paired with Guide metadata update.
- Rehow never passes an Attachment file itself to an external opener. It can render bounded decoded raster images, show inert text, export through a native-selected destination, or reveal the containing folder. `.desktop`, script, executable-bit, unknown, and active document types are text-only/reveal-only. Stored `media_type` is an untrusted hint; behavior derives from bounded byte sniffing and allowlists, with `nosniff` on any served bytes.

Phase 3 freezes product-facing import, rendering MIME, decoded-pixel, and lower usability limits. Those limits may be stricter than the Phase 1 scan hard caps below, but may not exceed them without Architect and Security Reviewer approval.

## Phase 1 parser and scan limits

These are hard v1 defaults; whole-input caps are enforced before parser allocation, and event-level caps are checked immediately after each bounded-input parser event. Changing them requires Architect/Security review:

- manifest or Category descriptor: 256 KiB each, including an optional byte-zero BOM;
- Guide file: 2 MiB total; frontmatter: 256 KiB from byte zero through the (optional) separator line inclusive, including an optional BOM and delimiter lines;
- YAML nesting depth: 16 simultaneous collection frames with the root mapping at depth 1; mapping entries: 256 completed key/value pairs per map; sequence entries: 2,048 completed child values; total YAML nodes: 20,000 scalar/sequence-start/mapping-start events including parser-generated empty/null scalars; generic scalar: 64 KiB each for decoded UTF-8 bytes and raw source-span bytes (tighter field limits still apply);
- YAML numeric normalization per document: 16 Mi checked closed-form work units computed exactly as ADR 0011's `R + 4*S + 4*C`, and 80 KiB total stored canonical numeric digits; one-over returns `yaml_numeric_work_limit` without a partial scalar/tree;
- YAML semantic-key work per document: 8 MiB total cached canonical-key bytes and 32 MiB total bytes examined by fallible metered key sorting/indexing/comparison; one-over returns `yaml_semantic_work_limit` without a partial mapping;
- Markdown AST: 100,000 semantic nodes and 128 simultaneous container frames; parser input remains capped at 2 MiB;
- managed path: 1,024 UTF-8 bytes; one filename: 255 bytes; only documented directory depths;
- Workspace: 1,000 Categories, 10,000 Guides, and 2 GiB total managed Markdown/YAML bytes during a bounded scan. Loaded views retain Guide metadata but lazily reread revision-checked bodies; disposable normalized search records are anonymous disk-backed state rather than a second in-memory content copy. Search derivation permits at most 64 MiB per encoded Guide record, 65,536 values per repeated field, and 4 GiB across physically live prior plus candidate backings, failing the candidate refresh before a one-over write. Compaction starts only with at most 2 GiB of prior backings;
- Attachment inventory: 10,000 regular-file candidates, 64 MiB per candidate, and 4 GiB aggregate candidate bytes. Enumeration counts candidates before content hashing, hashes through a bounded stream, and never allocates a whole Attachment from its declared size.

An oversized manifest makes inspection `malformed`; an oversized Category/Guide is isolated with `FILE_TOO_LARGE`. An Attachment over the per-file cap is isolated with `FILE_TOO_LARGE`, makes the full inventory incomplete, and disables broad mutation while unrelated revision-scoped reads remain available. Exceeding any Workspace or Attachment count/aggregate scan limit stops before the next allocation and opens the Workspace read-only with `WORKSPACE_LIMIT_EXCEEDED`; Rehow never silently ignores the remainder. Phase 3 must add MIME, decoded-pixel, and import limits before Attachment content can be displayed or copied.

ADR 0011 defines exact bounded event counting, YAML 1.2 scalar resolution, delimiter/BOM/newline handling, semantic/presentation reconciliation, and lossless span-edit refusal. It must be accepted before parser implementation begins.

## Adoption and import

### Copy import (v1 required)

Scan a user-selected ordinary collection without following symlinks, show included/excluded/malformed/oversize files, choose/create a destination Workspace and Category mapping, then copy each Markdown file into a new draft Guide. Original files are untouched. Imported source frontmatter/body is preserved where safe; a provenance note contains a relative display path, not a trusted executable link.

### Per-file adoption of hand-written Guides

See “Hand-written (unadopted) Guides” above and ADR 0063. This applies only to Markdown files inside an existing Workspace Category and only on an explicit save.

### In-place adoption (decision deferred)

Deferred from v1 by ADRs 0050 and 0051. Adoption would add a manifest/Category descriptors and modify or move Markdown. It must produce an exact dry-run, backup, collision list, schema round-trip warning, and explicit confirmation. If any file cannot round-trip safely, offer copy import only. Never offer adoption for an unrelated nonempty folder without suitable Markdown.

## Migration rules

- Schema v1 is the first released format, so its supported-older-version registry is empty and no migration apply action is exposed.
- Every migration has source/target versions, preconditions, deterministic transform, fixture tests, backup verification, journaled steps, post-validation, and documented rollback by backup restore.
- Migrations do not cross unsupported versions implicitly.
- Unknown metadata survives unless an ADR explicitly deprecates a conflicting key and the migration preserves it under a namespaced extension.
- Newer schema is never “best effort” writable.

## Validation severity

- **error:** unsafe/unparseable/identity/schema condition; excludes the item from mutation and normal index.
- **warning:** readable but incomplete/dangling/stale condition; remains usable with UI explanation.
- **info:** recommendation such as missing rollback section.

Diagnostics have stable code, relative file (when safe), JSON-pointer-like field, message, recovery action, and optional source span. They never echo secrets or entire Guide bodies.
