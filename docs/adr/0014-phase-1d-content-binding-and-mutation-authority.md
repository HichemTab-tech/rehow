# ADR 0014: Phase 1D Content Binding and Mutation Authority

- Status: Accepted for Phase 1D-A and Phase 1D-B; Phase 1D-C remains proposed
- Date: 2026-09-07
- Owners: Architect and Security Reviewer
- Scope: Phase 1D only

## Context

ADRs 0011–0013 prove a bounded generic YAML source, presentation, semantic, and reconciliation artifact. They deliberately do not decide which entries are Rehow fields, construct product aggregates, or authorize edits. Phase 1D closes that gap without acquiring filesystem, revision, clock, entropy, rendering, process, network, logging, Tauri, or MCP authority.

## Decision

### Delivery sequence

To keep the foundation reviewable, Phase 1D is split without changing its final scope:

1. **Phase 1D-A (accepted now):** read-only `WorkspaceManifest`, `WorkspaceSettings`, and `Category` records; current/newer Manifest schema classification; exact known/unknown binding; sanitized private failures; no new dependency.
2. **Phase 1D-B (accepted):** Guide, Applicability, Reference, and Attachment binding after the three dependency audits.
3. **Phase 1D-C1 (implemented checkpoint; final review pending):** deterministic canonical emission for new Manifest, Category, and draft Guide documents.
4. **Phase 1D-C2 (proposed):** private mutation candidates over existing source bytes. Phase 1D-C is accepted only after the combined C1/C2 review and full gate.

The read-only binding parts through 1D-B are implementation authority. Emitter and mutation text records the intended 1D-C direction and remains proposed until its combined architecture/security review. No Phase 1D subphase writes files.

### Domain and binding boundary

`rehow-core::domain` exposes read-only records named `WorkspaceManifest`, `WorkspaceSettings`, `Category`, `Guide`, `Applicability`, `GuideReference`, `Attachment`, `VerificationMetadata`, and `ArchiveMetadata`. It exposes `CategoryStatus::{Active, Archived}`, `GuideStatus::{Draft, Tested, Stale, Archived}`, and `RestorableGuideStatus::{Draft, Tested, Stale}`. The Guide body remains exact opaque CommonMark/GFM bytes; section parsing and readiness validation remain Phase 3.

Fields are private and aggregates have read-only accessors. There are no public aggregate constructors, public fields, mutable accessors, `Default`, Serde, TypeScript, hash/order implementations, or public generic mutation APIs. Status enums may implement copy/equality/order/hash and a manual non-content `Debug`; aggregates and content-bearing values do not gain content-revealing `Debug` or `Display`.

The public accessor surface is exact. Scalar newtypes are returned by shared reference; text is `&str`; collections are slices; and optional source presence remains `Option`:

```rust
impl WorkspaceManifest {
    fn schema_version(&self) -> u32;
    fn workspace_id(&self) -> &WorkspaceId;
    fn name(&self) -> &str;
    fn created_at(&self) -> &Timestamp;
    fn updated_at(&self) -> &Timestamp;
    fn settings(&self) -> Option<&WorkspaceSettings>;
}
impl WorkspaceSettings { fn default_category_id(&self) -> Option<&CategoryId>; }
impl Category {
    fn schema_version(&self) -> u32; fn id(&self) -> &CategoryId;
    fn name(&self) -> &str; fn summary(&self) -> Option<&str>;
    fn order(&self) -> i64; fn status(&self) -> CategoryStatus;
    fn created_at(&self) -> &Timestamp; fn updated_at(&self) -> &Timestamp;
}
impl Guide {
    fn schema_version(&self) -> u32; fn id(&self) -> &GuideId;
    fn title(&self) -> &str; fn summary(&self) -> &str;
    fn category_id(&self) -> &CategoryId;
    fn tags(&self) -> &[String]; fn aliases(&self) -> &[String];
    fn declared_status(&self) -> GuideStatus;
    fn created_at(&self) -> &Timestamp; fn updated_at(&self) -> &Timestamp;
    fn verification(&self) -> Option<&VerificationMetadata>;
    fn archive(&self) -> Option<&ArchiveMetadata>;
    fn applies_to(&self) -> Option<&[Applicability]>;
    fn prerequisites(&self) -> Option<&[String]>;
    fn warnings(&self) -> Option<&[String]>;
    fn files_affected(&self) -> Option<&[String]>;
    fn related_guide_ids(&self) -> Option<&[GuideId]>;
    fn references(&self) -> Option<&[GuideReference]>;
    fn attachments(&self) -> Option<&[Attachment]>;
    fn body(&self) -> &[u8];
}
```

Nested records expose only their named known members with the same `&T`/`&str`/`Option` convention: `Applicability::{platform,versions,notes}`, `GuideReference::{title,url}`, `Attachment::{id,path,title,media_type,sha256,size_bytes}`, `VerificationMetadata::{last_verified_at,verified_content_hash}`, and `ArchiveMetadata::{archived_from_status,archived_at}`. `size_bytes` is `Option<u64>` and enum accessors return by value. Unknowns are not cloned into public aggregates or publicly mutable; the bound artifact retains the complete private partition.

The following binders and their owning artifacts are crate-private:

```rust
bind_workspace_manifest(ParsedYamlDocument) -> Result<BoundWorkspaceManifest, BindFailure>
bind_category(ParsedYamlDocument) -> Result<BoundCategory, BindFailure>
bind_guide(ParsedGuideSource) -> Result<BoundGuide, BindFailure>
```

Each bound document owns the exact parsed artifact/source, typed aggregate, complete known/unknown partition, safe source coordinates, and an edit index only when `RoundTripStatus::Proven`. A `ReadOnly` artifact may yield a validated read view but never mutation capability. Phase 1D removes Phase 1C's parser-module `dead_code` allowance once production binding consumes it.

Bound types expose crate-private `record(&self)`, `source_bytes(&self)`, `unknown_partition(&self)`, and `mutation_capability(&self) -> Option<...>` only. The capability borrows/is tied to that exact owner and cannot outlive or be transferred to another artifact. A candidate exposes crate-private `bytes(&self)`, `record(&self)`, and `effects(&self)`; it exposes neither raw spans nor mutable bytes.

The retained-allocation inventory is fixed: the parsed source/artifact remains one owner; a bound record may copy each known decoded scalar once and a Guide body once, but never copies unknown semantic subtrees. Case-fold keys exist only during binding. One candidate adds one output byte buffer plus one rebound record/artifact; no history of candidates or edit buffers is retained. Every Rehow-owned vector/string uses checked length arithmetic and `try_reserve_exact` before append. The body copy and every derived/candidate byte are charged by the resource meter below; peak retained product copies are therefore at most the original accepted source plus one bound known/body projection plus one candidate source plus one rebound known/body projection.

### Known and unknown fields

Known keys are the exact decoded Unicode scalar sequence listed by schema v1. Matching performs no case folding or normalization. Case-different or canonically equivalent spellings are unknown extensions; an exact reserved key with the wrong type or value is a hard validation error. Semantic duplicate keys remain parser failures.

Every root entry is partitioned exactly once as known or unknown by semantic entry identity. Unknown root entries and unknown string-keyed children of `settings`, `applies_to`, `references`, and `attachments` are preserved with their exact generic semantic descendants and source bytes. Complex keys in these fixed product child mappings are invalid; complex keys inside an unknown value remain allowed. Phase 1D exposes no unknown-field mutation API. A future schema may reserve a new exact key only through a migration ADR; schema v1 has no wildcard or future-reserved prefix.

### Text, Unicode, and collections

All validated text must already be trimmed: the binder never trims or normalizes stored text. The exact trim set is Unicode White_Space from Unicode 16.0.0: `U+0009..U+000D`, `U+0020`, `U+0085`, `U+00A0`, `U+1680`, `U+2000..U+200A`, `U+2028`, `U+2029`, `U+202F`, `U+205F`, and `U+3000`. Lengths below count Unicode scalar values. Unicode noncharacters are rejected. Other interior controls and `U+FEFF` are retained; deterministic emission escapes them.

| Field | Presence and bound |
| --- | --- |
| Workspace/Category `name`, Guide `title` | required string, 1–200 |
| Category `summary` | optional string, 1–500 when present |
| Guide `summary` | required string, 1–500 |
| `tags` | required sequence, 0–64; items 1–64 |
| `aliases` | required sequence, 0–64; items 1–200 |
| `applies_to` | optional sequence, default empty, 0–32 |
| `prerequisites`, `warnings` | optional sequences, 0–128; items 1–1,000 |
| `files_affected` | optional sequence, 0–256; items 1–1,024 and at most 4,096 UTF-8 bytes |
| `related_guide_ids` | optional sequence, 0–256 unique Guide IDs |
| `references` | optional sequence, 0–128 |
| `attachments` | optional sequence, 0–256 |

An optional collection may be explicitly empty. YAML null is never omission. Public accessors preserve that distinction: absent is `None`, an explicit empty sequence is `Some(&[])`; mutations use the same `Option<Vec<_>>` rule. Required Tags/Aliases always exist and use `[]` for empty. `settings` is `None` when absent and `Some` when explicitly present, even if it contains only unknown children.

Tags and aliases are independently unique under locale-independent full default Unicode case folding using Unicode 16.0.0 data, without normalization. Comparison uses a bounded deterministic sort keyed by folded value and encounter index; a later collision fails. Thus `Maße` and `MASSE`, and final/ordinary sigma equivalents, collide; Turkic tailoring is not used, while precomposed `é` differs from `e` plus combining acute. Phase 1D pins and audits `focaccia = "=2.0.0"` with default features disabled unless implementation proof shows it cannot supply this exact frozen contract, which requires Architect escalation before substitution.

### Nested value grammar

`Applicability` is a mapping with required `platform` text 1–100, optional `versions` text 1–200, optional `notes` text 1–500, and preserved unknown string-keyed members. Duplicate platform values are allowed.

`GuideReference` is a mapping with required `title` text 1–200 and required `url` of 1–2,048 UTF-8 bytes, plus preserved unknown string-keyed members. The URL string is retained exactly. Strict checks operate on the original bytes before pinned `url = "=2.5.8"` parsing; parser serialization is not compared because valid WHATWG serialization may add a root slash. The scheme is exact lowercase `http`, `https`, or `mailto`. Reject surrounding trim, backslash, non-ASCII raw bytes, incomplete/lowercase percent escapes, percent-decoded C0/C1/DEL (`00..1F`, `7F..9F`), encoded backslash, relative URLs, and every other scheme.

For HTTP(S), the original authority contains no percent, userinfo, empty host, or empty port. A DNS host is lowercase ASCII, at most 253 bytes, has no trailing dot or empty label, and has 1–63-byte labels that begin/end alphanumeric and otherwise use only `[a-z0-9-]`; underscore and IDNA input are rejected. Canonical IPv4 is exactly four decimal 0–255 components with no leading zero. Canonical IPv6 is bracketed lowercase RFC 5952 with no zone and must equal Rust's canonical IPv6 display inside the brackets. A present port is ASCII decimal without a leading zero, in 1–65,535, and is not the scheme default (`80` for HTTP, `443` for HTTPS). For `mailto`, the original suffix has no `//`, fragment, or empty path; a query is allowed. These rules deliberately reject uppercase scheme/host, Unicode/percent hosts, trailing dots, `127.1`, zero-padded IPv4, `:080`, explicit default ports, and expanded/uppercase IPv6. Phase 1D never opens or resolves a URL; later openers reparse and display the destination.

`Attachment` is a mapping with required UUIDv7 `id`, `path`, trimmed `title` of 1–200 scalars and at most 800 UTF-8 bytes, and exact 64-character lowercase-hex `sha256`; optional `media_type` and semantic integer `size_bytes` in `0..=u64::MAX`; and preserved unknown string-keyed members. IDs and paths are independently unique within a Guide. The path is exactly `attachments/<current-guide-id>/<filename>`, exactly three slash-separated components, never URL-decoded or shell-expanded, and at most 1,024 UTF-8 bytes. The filename is one NFC UTF-8 component of 1–255 bytes, not `.` or `..`, with no NUL, slash, backslash, C0/C1 control, bidi formatting/control, or `U+FEFF`; this filename-specific restriction is intentionally stricter than ordinary display text. Phase 1D pins and audits `unicode-normalization = "=0.1.25"` with default features disabled.

Media type is an untrusted display hint: lowercase, parameter-free ASCII `type/subtype`, 3–127 bytes, one slash, each side nonempty, and characters limited exactly to `[a-z0-9!#$&^_.+-]`. Digest, size, and media type grant no read or allocation authority. Phase 1E correlates the inventory and streams confined regular files.

### Scalar types, schema compatibility, and invariants

IDs, timestamps, enums, URLs, hashes, paths, and text are YAML strings. Schema versions, Category order, and Attachment size are semantic YAML integers. Collections are sequences and structured values are mappings. Manifest `schema_version` is a positive `u32`; schema 1 is the only writable version and there is no supported older version yet. `inspect_manifest_schema(&ParsedYamlDocument)` views only the already accepted semantic artifact—never raw rescanning or another parser—and returns the closed `CurrentV1 | NewerUnsupported(u32)` classification. Missing, wrong-kind, non-positive, or above-`u32` values are malformed before compatibility classification; `1` is current and `2..=u32::MAX` is newer unsupported. Only `CurrentV1` enters the v1 binder. Category and Guide schema versions must equal 1. Category order is `i64`; Attachment size is `u64`.

For every document, `updated_at >= created_at`. Verification fields occur as a pair. Draft forbids them; tested requires them; stale permits either both or neither; archived follows `archived_from_status`. When present, `created_at <= last_verified_at <= updated_at`.

Archive fields also occur as a pair. Archived requires `archived_from_status` plus `archived_at`; non-archived forbids both. The prior status is draft/tested/stale and `created_at <= archived_at <= updated_at`. A Guide cannot relate to itself. Existence of Category, related Guide, and Attachment files and duplicate IDs across a Workspace remain Phase 1E inventory validation.

### Tested-state trust

`GuideStatus::Tested` represents only the declaration stored in frontmatter. Phase 1D exposes it as `declared_status`; it provides no `is_tested` or effective-trust accessor. Until Phase 3 recomputes and validates the accepted `verify:v1` content hash, a stored tested declaration is unverified and must be treated as effectively stale by trust consumers. A malformed verification pair is a validation error, not an extension.

No Phase 1D emitter or mutation accepts a generic status, tested transition, or caller-supplied verification fields. New/imported/AI-authored Guides are draft. Verification-relevant edits stale a tested declaration for every origin; automation/MCP edits also stale tested for organizational changes. Native organizational edits may preserve a trusted tested state only after Phase 3 introduces the sealed approval capability. Automation archives a tested declaration as `archived_from_status: stale`. Origin is a sealed core capability, never a caller Boolean.

### Binding failures and safe schema paths

`BindFailure` is crate-private and contains only document kind, one closed kind (`MissingRequiredField`, `InvalidFieldType`, `InvalidFieldValue`, `DuplicateField`, `ResourceLimit`, or `BindingInvariant`), a closed known-field path, optional source position, and an optional already-validated parsed identity. It stores no rejected content, unknown key, URL, filename, body, YAML fragment, dependency error, or arbitrary path. Newer-schema classification is a separate successful `NewerUnsupported(u32)` header result and never a partial aggregate.

Failure order is deterministic: complete-root invariant; fields in schema order; each collection in encounter order and each nested member in schema order; and a cross-field invariant as soon as its final input has been validated. The binder may parse Category/Guide identity non-authoritatively for optional public error detail without changing precedence. Missing fields use `MissingRequiredField` and no position; wrong YAML kinds and non-mapping items use `InvalidFieldType`; lexical, bound, uniqueness, and cross-field failures use `InvalidFieldValue`; a repeated known member that survives generic duplicate defense uses `DuplicateField`; allocation refusal uses `ResourceLimit`; impossible artifact/partition disagreement alone uses `BindingInvariant`.

Phase 1D-B retains the closed root-only `SchemaFieldPath` catalog from ADR 0012. A nested failure names its containing root collection and its proven root value position; it never turns an untrusted key or index into a schema path. More granular item locations may be added only through a later protocol ADR and are not required for safe Workspace inspection.

Phase 1E/application code maps private field failures to the applicable `INVALID_MANIFEST`, `INVALID_CATEGORY`, or `INVALID_GUIDE` with diagnostic codes `missing_required_field`, `invalid_field_type`, `invalid_field_value`, `duplicate_field`, or `content_resource_limit`. `BindingInvariant` maps only to `INTERNAL` with a fresh incident ID and no field/position. Newer manifests map to `SCHEMA_UNSUPPORTED`; no aggregate is returned. Parser failures retain the ADR 0013 mappings. All mapping is exhaustive and content-free.

### Closed mutation authority

The crate-private intent enums are exact:

```rust
enum WorkspaceMutation { SetName(String), SetDefaultCategory(Option<CategoryId>) }
enum CategoryMutation {
    SetName(String), SetSummary(Option<String>), SetOrder(i64), Archive, Restore,
}
enum GuideMutation {
    SetTitle(String), SetSummary(String), ChangeCategory(CategoryId),
    ReplaceTags(Vec<String>), ReplaceAliases(Vec<String>),
    ReplaceApplicability(Option<Vec<Applicability>>),
    ReplacePrerequisites(Option<Vec<String>>), ReplaceWarnings(Option<Vec<String>>),
    ReplaceFilesAffected(Option<Vec<String>>),
    ReplaceRelatedGuides(Option<Vec<GuideId>>),
    ReplaceReferences(Option<Vec<GuideReference>>), ReplaceBody(Vec<u8>),
    MarkStale, Archive, Restore,
}
fn plan_workspace_mutation(&BoundWorkspaceManifest, WorkspaceMutation, &MutationStamp)
    -> Result<MutationOutcome<BoundWorkspaceManifest>, MutationFailure>;
fn plan_category_mutation(&BoundCategory, CategoryMutation, &MutationStamp)
    -> Result<MutationOutcome<BoundCategory>, MutationFailure>;
fn plan_guide_mutation(&BoundGuide, GuideMutation, &MutationStamp)
    -> Result<MutationOutcome<BoundGuide>, MutationFailure>;
enum MutationOutcome<T> { NoOp, Candidate(MutationCandidate<T>) }
```

Payload records are validated afresh; callers cannot construct unchecked aggregate state. For optional fields/collections, `None` means remove/omit and `Some(empty)` means retain/emit an explicit empty sequence. Removing an already absent value and writing a byte/semantic-equal value are no-ops. Required values cannot be removed. Attachment metadata is a separate sealed later journal operation. No generic field/YAML/node/range/effect mutation exists.

`MutationStamp` contains only a trusted timestamp. Its constructor is crate-private to Phase 1E application code and a `cfg(test)` seam; it conveys no tested-preservation authority. Every Phase 1D/1E mutation is treated as automation/untrusted for declared-tested invalidation. Only a later Phase 3 ADR may add a separate non-forgeable native preservation capability bound to exact Guide ID, revision, and accepted verification hash; no Boolean or bare origin enum can confer it. A no-op retains exact bytes, revision semantics, timestamps, and status. An actual mutation sets `updated_at = max(current updated_at, stamp.at)`; archive operations set `archived_at` to that same resulting updated timestamp.

Title, summary, applicability, prerequisites, warnings, files, related Guides, references, body, and Attachment metadata are verification-relevant. Category, tags, and aliases are organizational only for a future Phase 3 trusted capability; Phase 1D/1E still stale them. Any actual Guide field edit changes declared `tested` to `stale`; while archived it changes `archived_from_status: tested` to `stale` and leaves `status: archived`/`archived_at` unchanged. Verification fields remain unchanged.

The status matrix is exact. `MarkStale` maps draft or tested to stale, is a no-op on stale, maps archived-from-draft/tested to archived-from-stale, and is a no-op on archived-from-stale. `Archive` maps draft/stale to archived with that prior value; tested maps to archived-from-stale; already archived is a no-op. `Restore` is invalid on non-archived Guides; archived-from-draft restores draft, archived-from-stale restores stale, and archived-from-tested restores stale in Phase 1D/1E; it removes both archive fields. Category `Archive` maps active to archived and is otherwise a no-op; Category `Restore` maps archived to active and is otherwise a no-op. Workspace mutations have no status transition. Every actual transition updates `updated_at`; Guide Archive/Restore/MarkStale retains the verification pair under its existing validation rules.

Category name does not rename its directory. Guide title does not rename its file. `ChangeCategory` returns only a validated candidate; Phase 1E owns the atomic move.

Mutation capability has a private constructor and is tied to the exact owned parsed artifact and document kind. Adapters can provide typed validated intent values only—not YAML, field names, node/entry IDs, offsets, ranges, insertion sites, or effect sets. Lookup uses the reconciled root-entry relationship. A known-key collision causes refusal; insertion never creates a semantic duplicate. The planner returns final candidate bytes or a crate-private candidate object, never reusable raw edit instructions.

### Concrete edit proof

Replacement covers the complete proven concrete value range, including an explicit tag; it refuses implicit/unreconciled values, comments contained by or ambiguously owned by the value, and any overlap. Collection replacement is allowed only for proven block style with no comments and no unknown descendant. Flow-collection wholesale edits are refused, with one narrow required-field exception: the exact untagged, comment-free empty sequence token `[]` emitted for new Guide `tags` and `aliases` may be replaced together with its proven complete root entry by the canonical block sequence form. This exception cannot accept a nonempty flow sequence, explicit tag, unknown descendant, comment, implicit value, or unreconciled range. Empty replacement emits `[]`; nonempty replacement emits a two-space-indented block sequence using the document's proven line-ending style.

Insertion requires a proven block-mapping zero-width line boundary, placed before the next greater schema-ranked field or after the final entry while preserving unknown relative order. Nested mappings use exactly two-space indentation. Insertions at one site coalesce in schema order. Removal covers the full block entry and its owned newline; final-entry preceding-newline behavior follows ADR 0011 and ambiguous relevant comments cause refusal. Clearing the only known `settings.default_category_id` removes `settings` only when it has no unknown child; otherwise only the nested entry is removed.

All companion edits are one atomic candidate. Checked, UTF-8-aligned, contained, pairwise-disjoint ranges are applied in descending order; at most eight edits are allowed. The candidate must reparse and rebind to the same kind and identity with unchanged schema/created timestamp, exact requested semantic effects, equal untouched known fields, iterative equality of every unknown descendant, and exact bytes outside authorized spans. A body edit changes only the body plus declared companions. Any disagreement returns no candidate and the original bytes remain unchanged.

`SemanticEffectSet` is crate-private, sorted by schema rank, deduplicated, capped at eight, contains only closed known-field paths/body effects, and names every automatic companion effect. Every actual mutation includes its target and `/updated_at`; status invalidation adds `/status` or `/archived_from_status`; Archive adds `/status`, `/archived_from_status`, `/archived_at`; Restore adds/removes those same three; a settings parent insertion/removal adds `/settings` and `/settings/default_category_id`; a body edit adds the private body effect. It never contains content.

`MutationFailure` is the exact crate-private taxonomy `InvalidValue(path) | InvalidTransition(path) | ReadOnly(path, reason) | CommentUnsafe(path) | PresentationUnsafe(path, reason) | ResourceLimit(path) | CandidateMismatch(path) | BindingInvariant`. Precedence is payload validation, legal-transition validation, no-op determination, artifact/read-only capability, target span/site, required companion spans/sites in schema order, edit-count/range checks, allocation/work bounds, candidate parse, candidate bind, identity/invariant comparison, effect comparison, then outside-span proof. Phase 1E maps the first two to `INVALID_ARGUMENT` (`invalid_format` or `unsupported_value`), the next four and `CandidateMismatch` to `UNSAFE_ROUND_TRIP` with closed reasons, `ResourceLimit` to `UNSAFE_ROUND_TRIP/content_resource_limit`, and `BindingInvariant` to `INTERNAL`. Round-trip reasons remain closed; any catalog addition must be made in the Phase 1D protocol diff and generated-contract evidence.

### Canonical new-document emitter

Phase 1D proves a fixed-schema emitter but grants no production creation authority. Phase 1E supplies sealed IDs and timestamps. New Manifest/Category timestamps are equal; Categories are active; Guides are draft with no verification/archive fields. Unknown fields are absent. Required Tags/Aliases emit as empty sequences; optional empty collections are omitted.

Emitter order is the order documented in `docs/content-model.md`. Strings are double quoted; mappings/sequences are block style; empty maps/sequences use `{}`/`[]`; output has no BOM, uses LF, and ends in exactly one LF. A Guide has exact delimiter lines and one blank separator, and its body is UTF-8 LF text without BOM ending in one LF. Allocation uses checked arithmetic and fallible reserve; emitted bytes are reparsed/rebound and compared with the intended aggregate before return.

### Resource bounds and Phase 1E seam

Read-only binding traverses the already bounded parser artifact and schema-capped collections. Most passes are linear. Related-Guide and Attachment uniqueness use at most 32,640 comparisons per 256-item collection, so their pairwise scans have a small fixed ceiling independent of file size. Rehow-owned retained output allocations reserve fallibly and report `ResourceLimit`; index conversion or artifact disagreement reports `BindingInvariant`. No separate binding work meter is needed on top of the parser's file/frontmatter/node/entry/scalar/work caps and these field caps. Phase 1D-C candidate/effect validation must define and test its own checked work bound before acceptance.

Phase 1D performs no path resolution, filesystem I/O, revision calculation, clock/entropy access, process/network call, logging, Markdown rendering, or adapter exposure. Phase 1E wraps a bound document in a confined path/revision capability, rereads current bytes, verifies revision/absence/effect state, reparses, and regenerates/revalidates the candidate immediately before atomic application. It never applies a cached caller-visible range plan.

## Required evidence

Phase 1D-B implementation evidence covers scalar/cardinality boundaries, wrong kinds/nulls, status/time companions, unknown roots/children, Unicode fold collisions without normalization, hostile/noncanonical URLs, Attachment traversal/NFC/hash/size/MIME cases, self/duplicate IDs, read-only binding, exact body preservation, source positions, content-free failures, and public API privacy. Phase 1D-C must add the mutation/emitter-specific comment, presentation, range, allocation-fault, candidate-rebind, no-op, outside-span, deterministic-emission, property, and secret-canary evidence before its own acceptance.

Compile-fail and source audits prove private constructors, no mutable aggregate/unknown access, no Serde content exposure, and no Phase 1D filesystem, process, network, logging, public parser, Tauri, or MCP authority. Each subphase runs its affected Cargo gate and one combined independent correctness/security review; the full Cargo/pnpm/contract/Tauri no-bundle gate remains mandatory at the complete Phase 1D boundary.

## Consequences

Phase 1D can read valid product documents and prove narrowly typed in-memory candidates while preserving source fidelity. It cannot yet create, move, or change files. The contract is intentionally conservative: comment-heavy or stylistically ambiguous valid documents remain readable but may require external editing.

Three exact dependencies are proposed because their semantics form part of the stored-content contract. Their lock/source/features/licenses/MSRV/unsafe/build/proc-macro/yank/advisory state must pass the precompile dependency gate before any manifest or lockfile edit. A failed audit blocks or returns to Architect review; it does not silently substitute a package or weaken Unicode/URL rules.
