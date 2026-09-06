# ADR 0012: Phase 1B Protocol Construction and Dependencies

- Status: Accepted — Reviewer/high and Security Reviewer/xhigh passed
- Date: 2026-09-06

## Context

The signed Phase 1 baseline (`a0a5a0b`, with progress ledger `a644bf1`)
deliberately stops before protocol values can be implemented. The public wire
spellings are documented, but construction authority, exact timestamp grammar,
UUID generation, token decoding, error-copy ownership, Serde rejection rules,
TypeScript correlation, and the only safe assembly path for unknown YAML values
were not frozen. Allowing a Worker to choose those details would create public
API, trust, and resource-limit decisions in implementation.

This decision covers only Phase 1B. ADR 0011 remains authoritative for YAML
source parsing, numeric source normalization, presentation reconciliation, and
span edits. This ADR neither reopens that decision nor advances Phase 1C–1E.

## Decision

### Exact dependency set

Phase 1B adds exactly these direct dependencies to `rehow-core`:

```toml
[dependencies]
base64 = { version = "=0.23.1", default-features = false, features = ["std"] }
time = { version = "=0.3.55", default-features = false, features = ["std", "formatting", "parsing"] }
uuid = { version = "=1.26.0", default-features = false, features = ["std"] }

[dev-dependencies]
serde_json = { version = "=1.0.151", default-features = false, features = ["std"] }
trybuild = { version = "=1.0.120", default-features = false }
```

Existing exact pins for `serde` 1.0.229 with `derive` and `ts-rs` 12.0.1 remain
unchanged. The Worker regenerates and reviews `Cargo.lock`; it
does not add a workspace dependency table or change any other manifest.

All five additions are maintained, widely used Rust crates under `MIT OR
Apache-2.0`, and support the project's Rust 1.98 baseline (`uuid` requires
1.85, `time` and `trybuild` 1.88, `base64` and `serde_json` 1.71). The runtime
set adds no native library, network client, async runtime, parser for Workspace
content, or entropy provider:

- `uuid` is used only for syntax/version/variant validation and formatting. The
  `v7`, `rng`, `rng-getrandom`, `rng-rand`, `serde`, and default feature sets are
  not enabled by `rehow-core`'s direct dependency edge. Other existing Workspace
  members may enable UUID features through Cargo feature unification; that does
  not grant a Rehow ID constructor or permit core calls/re-exports of generation
  APIs. Custom Serde is required so permissive UUID spellings cannot enter the
  domain.
- `time` is used only after a short exact ASCII preflight to validate a UTC
  Gregorian instant and format its canonical spelling. `large-dates`,
  `local-offset`, `macros`, random, and Serde features are not enabled by
  `rehow-core`'s direct dependency edge. Other Workspace members may unify
  additional `time` features, but core does not call or re-export those APIs.
  Version 0.3.55 is newer than the `>=0.3.47` fix for RUSTSEC-2026-0009. Rehow
  never invokes RFC 2822 parsing or local-offset discovery.
- `base64` decodes and re-encodes the fixed 43-byte plan payload. Defaults are
  disabled because 0.23.1's default includes `simd-unsafe`; the selected
  `std`-only build excludes that unsafe SIMD path. Rehow still performs its own
  length/alphabet/final-character checks and exact re-encode comparison.
- `serde_json` is test-only and supplies exact JSON fixtures. `trybuild` is
  test-only and proves that private constructors and representation types are
  inaccessible from an external crate. Neither is linked into production.

The crates.io metadata, upstream feature manifests, and RustSec package entries
were reviewed on 2026-09-06. No listed advisory affects the selected versions
and feature paths; notably, `time` 0.3.55 contains the stack-exhaustion fix and
the historical `base64` overflow affected versions older than 0.5.2. A lockfile
advisory finding before implementation acceptance blocks the packet rather than
being waived here.

The expected runtime size cost is limited to UUID parsing/formatting, UTC date
validation/formatting, and one Base64url engine; the packet records before/after
release sizes for `rehow-mcp` and the Tauri library artifact, without making a
size regression a hidden acceptance criterion. Manual UUID/date/Base64 code was
rejected because it would duplicate mature validation at a more sensitive
boundary. `chrono` is broader than the required UTC-only grammar. `rand` and
`getrandom` are unnecessary because generation is out of scope. `blake3`,
`hex`, `regex`, `anyhow`, `thiserror`, a YAML crate, a property-test crate, and a
snapshot-test crate are not added; the small fixed hex/token grammars and error
types are Rehow-owned manual code.

Every direct dependency remains exactly pinned. A version or feature change
requires a reviewed packet and a fresh advisory/license/API/lockfile review.

### Module boundary and trait surface

The implementation lives under the existing public `rehow_core::domain` and
`rehow_core::protocol` modules. `domain::yaml_value` and
`protocol::construction` are private modules. Public structs have private
fields. Representation enums, emitting catalogs, marker types used for literal
Serde/TypeScript values, and every unchecked constructor are private.

UUID, timestamp, decimal, digest, and closed discriminant values implement
`Clone`, `Copy`, `Eq`, `Ord`, and `Hash`. String-backed safe-display and token
values implement `Clone`, `Eq`, `Ord`, and `Hash`, but not `Copy`. `PlanToken`
implements only `Clone`, `Eq`, and `Hash` from that group; it is neither ordered
nor copied implicitly. `BoundedList<T>` and `ErrorDto` implement `Clone` and
`Eq` when their members do. Canonical YAML numbers implement `Clone` and `Eq`;
completed unknown YAML trees implement `Clone` but deliberately not ordinary
equality. `UnknownYamlEvent` is move-only. No public type implements traits
beyond those named by this ADR and the formatting/Serde/TS traits explicitly
specified below.

Values do not derive a content-bearing `Debug`. UUID/hash/token/error wrappers
have a fixed redacted `Debug`; `ErrorDto` reveals only its code and request ID.
`PlanToken` has neither `Display` nor a public secret-exposing accessor. Safe
display wrappers implement `Display`. Unknown YAML values/events do not
implement `Debug`, `Serialize`, `Deserialize`, `TS`, `Default`, `Hash`, or
`Ord`.

All public parse/build failures are fixed, non-content-bearing error values.
Their `Display` strings never include the rejected input. No failure type stores
source text.

All primitive fallible constructors and parsers other than the YAML event
builder return the same public opaque unit-like `ValueConstructionError`. It is
`Copy`, exposes only the fixed `Display` text `invalid canonical value`, and
implements `std::error::Error`; it has no kind, source chain, or rejected-value
field. Callers must not infer policy from a dependency-specific parse error.

### UUIDv7 values: parse-only in Phase 1B

Phase 1B defines distinct opaque newtypes for `WorkspaceId`, `CategoryId`,
`GuideId`, `AttachmentId`, `RequestId`, `ApprovalRequestId`, `OperationId`, and
`IncidentId`. It also defines `RecentWorkspaceId`, whose wire form has the
`recent:v1:` prefix. Every one is parse-only in Phase 1B; none is generated.

An ordinary ID accepts exactly 36 ASCII bytes in lowercase hyphenated form
`8-4-4-4-12`. Parsing with `uuid` must succeed, reformatting with its lowercase
hyphenated formatter must reproduce the input byte-for-byte, the version nibble
must be `7`, and the variant must be RFC 4122/9562 (`10` in the two most
significant variant bits). Uppercase, braces, URNs, simple form, whitespace,
other versions/variants, and trailing data are rejected. All UUIDv7 millisecond
payloads, including zero and the 48-bit maximum, are valid lexical IDs; product
time policies belong to the later generating operation. `RecentWorkspaceId`
applies the same validation after one exact lowercase `recent:v1:` prefix.

The public surface is `FromStr`, `TryFrom<&str>`, canonical `Display`, custom
string `Serialize`/`Deserialize`, and read-only comparison/hash traits. There is
no public `from_uuid`, `from_bytes`, mutable byte access, or UUID generator.
Serde never delegates to `uuid`'s Serde support. All ID TypeScript declarations
are named aliases of `string`, not numbers or a shared untyped Rust ID.

`PortableEntityId` is a public read-only discriminated value with four safe
constructors: `workspace(WorkspaceId)`, `category(CategoryId)`,
`guide(GuideId)`, and `attachment(AttachmentId)`. Its fields and representation
enum remain private, so a mismatched `kind`/ID cannot be constructed. Custom
deserialization rejects such a mismatch.

### No clock or entropy authority in Phase 1B

Phase 1B defines no `Clock`, `EntropySource`, random generator, monotonic UUID
context, system-time wrapper, or global/test override. It never calls
`SystemTime::now`, `OffsetDateTime::now_utc`, `getrandom`, or an OS random API.
Tests use parsed constants only.

Generation is deferred to the phase that owns each operation: Workspace,
Category, and Guide creation in Phase 1D/1E; Attachment import in Phase 3;
request/incident IDs at the application/adapter observability gate; operation
IDs at the ADR 0009 journal implementation; recent IDs in native preferences;
approval IDs and plan tokens with the approval/plan service. That decision must
inject fallible clock and entropy capabilities into the owning application
service, keep their production implementations crate-private, and provide
deterministic test implementations. It may not add a public `now`, public
`random`, ambient singleton, client-selected timestamp, or caller-selected
identifier. This deferral is intentional, not permission for a Worker to invent
generation in 1B.

### Timestamp grammar, canonicalization, and comparison

`Timestamp` is an opaque instant with exactly one accepted and emitted grammar:

```text
YYYY-MM-DDTHH:MM:SSZ
YYYY-MM-DDTHH:MM:SS.fractionZ
```

Every punctuation byte and `T`/`Z` is literal ASCII. The year is four digits in
`0001..=9999`; month/day must be a valid proleptic-Gregorian date; hour is
`00..=23`; minute and second are `00..=59`. Leap-second `60`, `24:00:00`, year
zero, signed/expanded years, spaces, lowercase `t`/`z`, numeric UTC offsets, and
trailing data are rejected. The optional fraction has 1–9 ASCII digits, must end
in `1..=9`, and therefore has no trailing zero. A zero fraction is omitted.
Examples: `2026-09-06T00:00:00Z`, `2026-09-06T00:00:00.1Z`, and
`2026-09-06T00:00:00.000000001Z` are canonical; `.0Z`, `.100Z`, and fractions
of 10 or more digits are rejected.

The preflight admits at most 30 bytes and validates the exact grammar before
calling `time`'s RFC 3339 parser. Reformatting the parsed UTC value through a
Rehow-owned formatter must reproduce the input byte-for-byte. Storage is a UTC
instant at nanosecond precision. `Eq`/`Ord` compare chronological instants, not
strings; callers must not use lexical string ordering. Public construction is
`FromStr`/`TryFrom<&str>` plus custom string
Serde and canonical `Display`; there is no public component constructor or
current-time constructor. TypeScript is `string`.

### Decimal, digest, and plan-token primitives

`U64Decimal::new(u64)` and `SignedInt64Decimal::new(i64)` are public and
infallible. Their public string parsers and custom Serde accept only the grammar
and bounds in `docs/error-contract.md`; Serde requires a JSON/string token and
never accepts a number. Both emit canonical strings and generate TypeScript
`string` aliases.

`Revision`, `Snapshot`, and `VerificationHash` are lexical parse-only wrappers
in Phase 1B. Each accepts its exact prefix (`rev:v1:`, `snapshot:v1:`, or
`verify:v1:`) followed by exactly 64 lowercase ASCII hexadecimal characters,
decodes to a private `[u8; 32]`, and re-emits the same canonical text. Uppercase,
wrong length/prefix, and non-hex are rejected. A small Rehow-owned checked nibble
loop performs decoding/encoding; no `hex` dependency is introduced. Each has
public `FromStr`, `TryFrom<&str>`, canonical `Display`, and custom string Serde.
Phase 1B
does not compute BLAKE3, create revisions/snapshots, calculate or semantically
validate a Guide verification hash, or expose `from_digest`. Lexically parsing a
`VerificationHash` does not establish `tested` status.

`PlanToken` is also parse-only. It requires the exact `plan:v1:` prefix and 43
payload characters matching `[A-Za-z0-9_-]`; the final character must be one of
`[AEIMQUYcgkosw048]`. Decode uses `base64`'s `URL_SAFE_NO_PAD` into exactly
`[u8; 32]`, and re-encoding must reproduce all 43 payload bytes. Padding,
standard-alphabet characters, whitespace, nonzero trailing pad bits, alternate
encodings, and all other lengths are rejected. Its bytes and canonical text are
private. It implements validated string Serde, equality, and hashing for a
future server-side single-use map, but no `Display`, public bytes/text accessor,
or generator. Serde serialization is the sole public wire exposure in 1B.

### Constructed tokens and public display values

`SchemaFieldPath`, `ReasonCode`, `DiagnosticCode`, `OperationCode`, and
`GitOperationCode` are opaque strings with private fields and no public
`new`/`parse`/`FromStr`/`TryFrom` constructor. Public access is read-only
`as_str`, custom Serde, and TypeScript `string`. Deserialization is a consumer
path: `SchemaFieldPath` accepts only the exact documented Phase 1B catalog;
the other four accept only 1–64 bytes matching
`[a-z][a-z0-9]*(?:_[a-z0-9]+)*`. Emission uses private closed enums in
`protocol::construction`; core error factories accept those enums rather than a
wire string. No parser, Git, OS, YAML key, or client string is converted to an
emitting token.

The Phase 1B `SchemaFieldPath` catalog is:

```text
/schema_version /workspace_id /name /created_at /updated_at
/settings /settings/default_category_id /id /title /summary /order /status
/category_id /tags /aliases /last_verified_at /verified_content_hash
/archived_from_status /archived_at /applies_to /prerequisites /warnings
/files_affected /related_guide_ids /references /attachments
/version /code /message /retryable /requestId /details /recovery
/expectedRevision /expectedSnapshot /planToken /operation /limit /cursor
```

The emitting reason catalog is:

```text
invalid_format required out_of_range unsupported_value capability_missing
capability_expired workspace_root_missing workspace_root_changed
workspace_not_writable path_absolute path_parent_component
path_invalid_component path_outside_workspace round_trip_unproven
presentation_mismatch comment_ownership_ambiguous plan_expired plan_replayed
plan_state_changed plan_scope_mismatch repository_owner_untrusted
repository_layout_unsupported journal_invalid journal_state_ambiguous
external_change_detected
```

The emitting diagnostic catalog is:

```text
missing_required_field invalid_field_type invalid_field_value duplicate_field
yaml_syntax yaml_depth_limit yaml_mapping_entry_limit yaml_sequence_entry_limit
yaml_node_limit yaml_scalar_limit yaml_numeric_work_limit
yaml_semantic_work_limit yaml_duplicate_key yaml_unsupported_construct
yaml_parser_disagreement invalid_frontmatter_envelope invalid_utf8
```

The emitting operation catalog is:

```text
workspace_open workspace_initialize workspace_migrate workspace_recover
category_create category_update category_reorder category_archive
category_restore category_remove guide_create guide_update
guide_change_category guide_change_status guide_archive guide_restore
guide_delete attachment_add attachment_remove search_rebuild
```

The emitting Git-operation catalog is:

```text
git_status git_init git_clone git_set_remote git_commit git_sync
```

Adding an emitting token is an explicit contract change in its owning phase.
Receiving a syntactically valid unknown reason/diagnostic/operation token remains
safe for forward-compatible display, but it does not add core authority.

`SafeRelativePath::parse_wire`, `SafeGitDisplay::parse_wire`, and
`SafeHost::parse_wire` are the only new public constructors for safe display
types; `FromStr`, `TryFrom<&str>`, and validated string Serde delegate to those
functions. These constructors validate an already escaped display value and do
not accept an OS path, Git output blob, URL, or authority.

For `SafeRelativePath`, decode each uppercase `%HH` triplet to bytes, join raw
UTF-8 bytes for unescaped scalars, then run the exact display encoder described
in `docs/error-contract.md` and require byte-identical re-encoding. This rejects
lowercase/incomplete escapes and noncanonical escapes such as `%41` or `%2F`.
The decoded form is nonempty, at most 1,024 bytes, and split by literal `/` into
nonempty segments that are neither `.` nor `..`; the display is at most 3,072
Unicode scalar values. `SafeGitDisplay` performs the same decode/re-encode
canonicality check without treating `/` structurally, requires nonempty output,
and permits at most 768 displayed Unicode scalar values. Raw prohibited scalars,
raw `%`, noncharacters, and invalid UTF-8 can appear only in their canonical
uppercase byte escapes. Neither type exposes decoded bytes in 1B.

`SafeHost::parse_wire` accepts exactly `unknown`, the DNS/IPv4/IPv6 forms in
`docs/error-contract.md`, and no trailing dot. It parses IP literals with
`std::net`, reformats them, and requires exact equality; DNS is preflighted
label-by-label. Unicode, uppercase, userinfo, brackets, zone IDs, port, path,
query, and fragment syntax are rejected. None of these wrappers is ever passed
to a filesystem or subprocess. IPv4-mapped IPv6 uses the Rust 1.98 canonical
`Ipv6Addr::to_string()` spelling: for example, `::ffff:192.0.2.128` is accepted
and the equivalent hexadecimal-tail spelling `::ffff:c000:280` is rejected.

`PublicText` has private fields, read-only `as_str`, string serialization, and a
`string` TypeScript representation. In Phase 1B it has no public constructor and
no standalone `Deserialize`; only the fixed catalog below can construct it.
`ErrorDto` custom deserialization has one private consumer-only validation path
that can reconstruct any nonempty text of at most 256 Unicode scalar values
which contains none of the prohibited scalars. It preserves such inbound text
on reserialization but grants no producer authority. A later reviewed sanitizer
may add a separate provenance-bearing producer path. It may not relax those
length or scalar rules.

### Fixed ErrorDto copy catalog

Every `ErrorDto` code owns exactly this Phase 1B message and recovery text.
These strings, plus the five Git failure reasons below, are the complete 1B
producer catalog for `PublicText`. They contain no interpolation.

| Code | Message | Recovery |
| --- | --- | --- |
| `INVALID_ARGUMENT` | The request contains an invalid value. | Correct the named field and try again. |
| `WORKSPACE_NOT_FOUND` | The recent Workspace could not be found. | Choose the Workspace again or remove it from recent Workspaces. |
| `WORKSPACE_UNAVAILABLE` | The Workspace is temporarily unavailable. | Restore access to the Workspace and retry. |
| `WORKSPACE_READ_ONLY` | The Workspace is read-only for this operation. | Reopen it with the required capability or choose a writable Workspace. |
| `WORKSPACE_LIMIT_EXCEEDED` | The Workspace exceeds a safety limit. | Reduce the named scope before retrying the operation. |
| `SCHEMA_UNSUPPORTED` | This Workspace schema version is not supported. | Open it read-only or use a Rehow version that supports this schema. |
| `INVALID_MANIFEST` | The Workspace manifest is invalid. | Correct the reported manifest field and inspect the Workspace again. |
| `INVALID_CATEGORY` | The Category descriptor is invalid. | Correct the reported Category field and refresh the Workspace. |
| `INVALID_GUIDE` | The Guide is invalid. | Correct the reported Guide field and refresh the Workspace. |
| `UNSAFE_ROUND_TRIP` | Rehow cannot prove that this edit preserves the document. | Edit the file externally or simplify the reported field, then refresh. |
| `DUPLICATE_ID` | More than one item uses the same identifier. | Give each reported item a distinct identifier before retrying. |
| `CATEGORY_NOT_FOUND` | The Category could not be found. | Refresh the Workspace and choose an existing Category. |
| `GUIDE_NOT_FOUND` | The Guide could not be found. | Refresh the Workspace and choose an existing Guide. |
| `ATTACHMENT_NOT_FOUND` | The Attachment could not be found. | Refresh the Guide and choose an existing Attachment. |
| `REVISION_CONFLICT` | The item changed since it was opened. | Read it again, compare changes, and retry with its new revision. |
| `SNAPSHOT_CONFLICT` | The Workspace changed after the operation was planned. | Refresh the Workspace, review the changes, and create a new plan. |
| `PATH_ESCAPE` | A managed path would leave the Workspace. | Correct the path and keep it within the opened Workspace. |
| `SYMLINK_DENIED` | A managed path contains a symbolic link. | Replace the symbolic link with a confined regular file and refresh. |
| `HARDLINK_DENIED` | A managed file has more than one hard link. | Copy it to a new regular file with one link and refresh. |
| `PERMISSION_DENIED` | The operation was denied by filesystem permissions. | Restore the required permission or choose another Workspace. |
| `FILE_TOO_LARGE` | The file exceeds the safety limit. | Reduce the file size before retrying. |
| `IO_BUSY` | The operation could not finish while the resource was busy. | Wait for the other operation to finish and retry. |
| `INDEX_NOT_READY` | The search index is not ready. | Wait for indexing to finish or rebuild the index. |
| `PLAN_STALE` | The operation plan is stale. | Refresh the Workspace and create a new plan. |
| `CONFIRMATION_REQUIRED` | This operation requires confirmation. | Review the exact plan in the native confirmation dialog. |
| `CONFIRMATION_INVALID` | The confirmation is invalid or no longer usable. | Refresh the Workspace and request a new confirmation. |
| `TEST_APPROVAL_REQUIRED` | Marking a Guide tested requires native user approval. | Try the exact Guide revision and approve it in the native app. |
| `GIT_UNAVAILABLE` | Git is unavailable. | Install or configure a trusted Git executable, then retry. |
| `GIT_UNSAFE_REPOSITORY` | Rehow cannot use this Git repository safely. | Correct the reported repository condition or use Git outside Rehow. |
| `GIT_AUTH_REQUIRED` | Git authentication is required. | Restore the reported credential source and retry. |
| `GIT_DIVERGED` | The local and remote Git histories have diverged. | Resolve the divergence with Git tooling, then refresh Rehow. |
| `GIT_CONFLICT` | The Git operation has conflicts. | Resolve the reported conflicts with Git tooling, then refresh Rehow. |
| `GIT_FAILED` | Git could not complete the operation. | Correct the reported Git condition and retry. |
| `MIGRATION_REQUIRED` | The Workspace must be migrated before it can be changed. | Review the migration plan and verified backup before continuing. |
| `RECOVERY_REQUIRED` | Rehow cannot safely complete or roll back an interrupted operation. | Keep the Workspace read-only and follow the recovery guidance. |
| `INTERNAL` | Rehow encountered an internal error. | Retry once; if it happens again, report the incident identifier. |

Producer-side `GIT_FAILED.sanitizedReason` is derived from `exitKind`, never
accepted from the caller: `spawn` → `Git could not be started.`; `exitStatus` →
`Git returned a non-success status.`; `signal` → `Git was terminated by a
signal.`; `timeout` → `Git did not finish before the timeout.`; and
`outputLimit` → `Git output exceeded the safety limit.` The consumer path
accepts and preserves any `sanitizedReason` that passes the same private
`PublicText` validation, so improved safe wording from a newer producer remains
forward compatible.

Changing this copy is a reviewed core change. UI, Tauri, and MCP adapters do not
replace it.

### Exhaustive ErrorDto construction

`ErrorDto` is an opaque public struct around a private exhaustive `ErrorKind`.
Every details struct is public only so `ts-rs` can name it; all fields are
private and it has no public constructor. `ErrorCode` and closed wire enums are
public read-only discriminants. `SourceLocation` owns paired nonzero one-based
`line`/`column`; callers cannot supply two independent options.

There is no public Rust constructor for `ErrorDto`. The private construction
module exposes exactly these `pub(super)` factories to the containing
`protocol` module, whose arguments map in the same order to the required and
then optional fields in
`docs/error-contract.md`:

```text
invalid_argument(request_id, field, reason)
workspace_not_found(request_id, recent_workspace_id)
workspace_unavailable(request_id, reason)
workspace_read_only(request_id, reason, required_capability?)
workspace_limit_exceeded(request_id, limit, observed_at_least, scope)
schema_unsupported(request_id, found_version, minimum_version, maximum_version)
invalid_manifest(request_id, relative_path, diagnostic, field?, location?)
invalid_category(request_id, relative_path, diagnostic, field?, location?, category_id?)
invalid_guide(request_id, relative_path, diagnostic, field?, location?, guide_id?)
unsafe_round_trip(request_id, relative_path, reason, field?)
duplicate_id(request_id, entity, relative_paths)
category_not_found(request_id, category_id)
guide_not_found(request_id, guide_id)
attachment_not_found(request_id, attachment_id, guide_id)
revision_conflict(request_id, entity, expected_revision, actual_revision)
snapshot_conflict(request_id, expected_snapshot, actual_snapshot, changed_entity_ids)
path_escape(request_id, reason, relative_path?)
symlink_denied(request_id, relative_path)
hardlink_denied(request_id, relative_path, link_count)
permission_denied(request_id, operation, relative_path?)
file_too_large(request_id, relative_path, limit_bytes, observed_at_least_bytes)
io_busy(request_id, operation, timeout_ms)
index_not_ready(request_id, index_generation, progress_completed, progress_total?)
plan_stale(request_id, operation, changed_entity_ids)
confirmation_required(request_id, operation, plan_token, expires_at)
confirmation_invalid(request_id, operation, reason)
test_approval_required(request_id, guide_id, guide_revision, approval_request_id)
git_unavailable(request_id, search_source_kinds)
git_unsafe_repository(request_id, reason)
git_auth_required(request_id, remote_name, host, credential_kind)
git_diverged(request_id, ahead, behind, branch)
git_conflict(request_id, conflict_kind, relative_paths)
git_failed(request_id, operation, exit_kind)
migration_required(request_id, found_version, target_version)
recovery_required(request_id, operation_id, reason, affected_relative_paths)
internal(request_id, incident_id)
```

The emitting catalog enums use the same `pub(super)` visibility, so no factory
signature has a more-visible private parameter and no construction API is
reachable from `application` or another core sibling. The construction module
has one narrowly scoped inner `#![allow(dead_code)]` during Phase 1B because the
complete producer catalog is intentionally exercised only by protocol unit
fixtures before owning application phases exist. Those later phases must add a
reviewed purpose-specific bridge or deliberately widen only the factories they
can safely produce; they may not expose arbitrary catalog/value construction.
Path- and Git-bearing factories remain inaccessible outside the protocol module
until their provenance gates pass. No broader lint allowance is permitted.

Every factory returns `Result<ErrorDto, ErrorConstructionError>`.
`ErrorConstructionError` is `pub(super)` so the factory signatures pass the
`private_interfaces` lint, but it is an opaque braced struct with a private unit
field, no public constructor, and no supplied value; it remains inaccessible
outside `protocol`. Construction and deserialization enforce these additional
correlations: schema minimum and maximum are positive with
minimum no greater than maximum and `foundVersion` is outside that inclusive
range; limit-exceeded observations are strictly greater than their limits;
duplicate-ID total count is at least two; expected and actual conflict tokens
differ; hard-link count is greater than one; busy timeout is nonzero; optional
index total is at least completed; Git divergence has nonzero ahead and behind
counts; unavailable-Git search sources and conflict paths have nonzero totals;
migration versions are positive with target greater than found; and
`SourceLocation` line and column are both nonzero. Other list
emptiness/order semantics belong to the producing phase and are not invented by
1B. Plan expiry cannot be compared with ambient time in 1B.

`entity` is `PortableEntityId`, so kind/typed-ID mismatch is impossible.
Factories accept private emitting catalog variants for field/reason/diagnostic/
operation values, not wire strings. Construction derives code, retryability,
message, recovery, sanitized Git reason, omission, and discriminants. The five
and only retryable variants are `WORKSPACE_NOT_FOUND`, `WORKSPACE_UNAVAILABLE`,
`IO_BUSY`, `INDEX_NOT_READY`, and `GIT_AUTH_REQUIRED`.

`BoundedList<T>::try_new(items, total_count)` is public and is the only
constructor/deserialization path. It enforces at most 32 items, exact `u32`
total, `items.len() == min(total_count, 32)`, and derives `truncated`; callers
never pass the flag. Accessors return `&[T]`, total count, and the derived flag.
Empty `0/0`, exact `32/32`, and truncated `33/32` and `40/32` are valid; a
shorter-than-available retained prefix, total below length, or more than 32
retained items is invalid.

### ErrorDto Serde and TypeScript contract

`ErrorDto` uses custom Serde. Serialization always emits exactly the seven
non-null top-level properties named in `docs/error-contract.md`; `version` is
the JSON integer `1`; code/details/retryability are correlated; optional detail
properties are omitted. It emits no unknown property and never switches on
`Serializer::is_human_readable`.

Deserialization:

- requires an object with each top-level property exactly once, rejects unknown
  top-level properties, wrong case, missing fields, and `null`;
- requires literal integer `1`, the exact known code, valid `PublicText` for
  message/recovery, and its exact retryability literal; safe noncatalog wording
  is accepted and preserved so copy can improve without a protocol break;
- requires `details` to be an object, rejects missing/duplicate known detail
  keys, rejects `null` for optional values, and rejects any property documented
  only for another code; paired line/column must both occur or both be absent;
- ignores other unknown detail properties for forward-compatible consumers,
  but never retains or re-emits them;
- applies every primitive, enum, bounded-list, entity-correlation, nonzero
  location, and count invariant before returning an `ErrorDto`.

Duplicate unknown detail keys are ignored because they are discarded; every
known key duplicate is rejected. JSON field order is immaterial. Deserialization
does not make an inbound error authoritative and exposes no error factory.
The fixed message, recovery, and Git-reason catalog is enforced for core
producer factories, not for compatible consumers.
The immediate per-code `details` object is the only object boundary that ignores
genuinely unknown properties. The top-level envelope, `BoundedList`,
`PortableEntityId`, and every other nested structured value reject unknown and
duplicate properties. Every known property at every depth rejects duplicates.

The Rust contract generator emits each primitive newtype as its named string
alias, closed discriminants as exact string unions, optional detail fields as
`?: T` without `null`, and `BoundedList<T>` with `items`, `totalCount`, and
`truncated`. `ErrorDto` must use the exact common-envelope intersection with the
exhaustive discriminated `ErrorVariant` union frozen below. Each `code` literal
selects exactly one detail type and a literal `true`/`false` retryability; it may
not generate independent `code: ErrorCode`, `retryable: boolean`, and
`details: ErrorDetails` fields. Its version is the literal type `1`.

Private Serde/TS marker types for `1`, `true`, and `false`, plus a private
internally tagged/flattened contract enum, are used to make `ts-rs` emit that
shape. Hand-editing generated TypeScript or keeping a second handwritten
wire schema is forbidden. Golden declarations and `tsc --noEmit` prove the
shape, including an exhaustive `switch` over all 36 codes and negative
`@ts-expect-error` cases for wrong code/detail/retryability pairings. Unknown
YAML values generate no TypeScript declaration.

The generated public TypeScript shape is frozen semantically; only whitespace,
line wrapping, and trailing punctuation are generator-owned. Declaration order
is the following exact sequence:

1. String aliases: `WorkspaceId`, `CategoryId`, `GuideId`, `AttachmentId`,
   `RequestId`, `ApprovalRequestId`, `OperationId`, `IncidentId`,
   `RecentWorkspaceId`, `Timestamp`, `U64Decimal`, `SignedInt64Decimal`,
   `Revision`, `Snapshot`, `VerificationHash`, `PlanToken`, `SchemaFieldPath`,
   `ReasonCode`, `DiagnosticCode`, `OperationCode`, `GitOperationCode`,
   `SafeRelativePath`, `SafeGitDisplay`, `SafeHost`, and `PublicText`.
2. Closed aliases: `RequiredCapability`, `WorkspaceLimitScope`,
   `GitSearchSourceKind`, `GitCredentialKind`, `GitExitKind`, `GitConflictKind`,
   and `ErrorCode`.
3. `PortableEntityId`, `BoundedList`, the 36 detail aliases below in error-code
   order, `ErrorEnvelope`, `ErrorVariant`, and `ErrorDto`.

Every string alias is exactly `string`, not a brand, object, or number.
`ErrorCode` is the exact union of the 36 `code` literals below.
`PortableEntityId` is exactly:

```ts
export type PortableEntityId =
  | { kind: "workspace"; id: WorkspaceId }
  | { kind: "category"; id: CategoryId }
  | { kind: "guide"; id: GuideId }
  | { kind: "attachment"; id: AttachmentId };
```

`BoundedList<T>` is exactly `{ items: Array<T>; totalCount: number;
truncated: boolean }`. The detail aliases, names, and member types are exactly
the following. The source-location union is inline in each of its three users;
it is not a separately exported declaration.

| Detail alias | Exact TypeScript members |
| --- | --- |
| `InvalidArgumentDetails` | `{ field: SchemaFieldPath; reason: ReasonCode }` |
| `WorkspaceNotFoundDetails` | `{ recentWorkspaceId: RecentWorkspaceId }` |
| `WorkspaceUnavailableDetails` | `{ reason: ReasonCode }` |
| `WorkspaceReadOnlyDetails` | `{ reason: ReasonCode; requiredCapability?: RequiredCapability }` |
| `WorkspaceLimitExceededDetails` | `{ limit: U64Decimal; observedAtLeast: U64Decimal; scope: WorkspaceLimitScope }` |
| `SchemaUnsupportedDetails` | `{ foundVersion: number; minimumVersion: number; maximumVersion: number }` |
| `InvalidManifestDetails` | `{ relativePath: SafeRelativePath; diagnosticCode: DiagnosticCode; field?: SchemaFieldPath } & ({ line: number; column: number } \| { line?: never; column?: never })` |
| `InvalidCategoryDetails` | `{ relativePath: SafeRelativePath; diagnosticCode: DiagnosticCode; field?: SchemaFieldPath; categoryId?: CategoryId } & ({ line: number; column: number } \| { line?: never; column?: never })` |
| `InvalidGuideDetails` | `{ relativePath: SafeRelativePath; diagnosticCode: DiagnosticCode; field?: SchemaFieldPath; guideId?: GuideId } & ({ line: number; column: number } \| { line?: never; column?: never })` |
| `UnsafeRoundTripDetails` | `{ relativePath: SafeRelativePath; reason: ReasonCode; field?: SchemaFieldPath }` |
| `DuplicateIdDetails` | `({ entityKind: "workspace"; entityId: WorkspaceId } \| { entityKind: "category"; entityId: CategoryId } \| { entityKind: "guide"; entityId: GuideId } \| { entityKind: "attachment"; entityId: AttachmentId }) & { relativePaths: BoundedList<SafeRelativePath> }` |
| `CategoryNotFoundDetails` | `{ categoryId: CategoryId }` |
| `GuideNotFoundDetails` | `{ guideId: GuideId }` |
| `AttachmentNotFoundDetails` | `{ attachmentId: AttachmentId; guideId: GuideId }` |
| `RevisionConflictDetails` | `({ entityKind: "workspace"; entityId: WorkspaceId } \| { entityKind: "category"; entityId: CategoryId } \| { entityKind: "guide"; entityId: GuideId } \| { entityKind: "attachment"; entityId: AttachmentId }) & { expectedRevision: Revision; actualRevision: Revision }` |
| `SnapshotConflictDetails` | `{ expectedSnapshot: Snapshot; actualSnapshot: Snapshot; changedEntityIds: BoundedList<PortableEntityId> }` |
| `PathEscapeDetails` | `{ reason: ReasonCode; relativePath?: SafeRelativePath }` |
| `SymlinkDeniedDetails` | `{ relativePath: SafeRelativePath }` |
| `HardlinkDeniedDetails` | `{ relativePath: SafeRelativePath; linkCount: U64Decimal }` |
| `PermissionDeniedDetails` | `{ operation: OperationCode; relativePath?: SafeRelativePath }` |
| `FileTooLargeDetails` | `{ relativePath: SafeRelativePath; limitBytes: U64Decimal; observedAtLeastBytes: U64Decimal }` |
| `IoBusyDetails` | `{ operation: OperationCode; timeoutMs: number }` |
| `IndexNotReadyDetails` | `{ indexGeneration: U64Decimal; progressCompleted: number; progressTotal?: number }` |
| `PlanStaleDetails` | `{ operation: OperationCode; changedEntityIds: BoundedList<PortableEntityId> }` |
| `ConfirmationRequiredDetails` | `{ operation: OperationCode; planToken: PlanToken; expiresAt: Timestamp }` |
| `ConfirmationInvalidDetails` | `{ operation: OperationCode; reason: ReasonCode }` |
| `TestApprovalRequiredDetails` | `{ guideId: GuideId; guideRevision: Revision; approvalRequestId: ApprovalRequestId }` |
| `GitUnavailableDetails` | `{ searchSourceKinds: BoundedList<GitSearchSourceKind> }` |
| `GitUnsafeRepositoryDetails` | `{ reason: ReasonCode }` |
| `GitAuthRequiredDetails` | `{ remoteName: SafeGitDisplay; host: SafeHost; credentialKind: GitCredentialKind }` |
| `GitDivergedDetails` | `{ ahead: U64Decimal; behind: U64Decimal; branch: SafeGitDisplay }` |
| `GitConflictDetails` | `{ conflictKind: GitConflictKind; relativePaths: BoundedList<SafeRelativePath> }` |
| `GitFailedDetails` | `{ operation: GitOperationCode; exitKind: GitExitKind; sanitizedReason: PublicText }` |
| `MigrationRequiredDetails` | `{ foundVersion: number; targetVersion: number }` |
| `RecoveryRequiredDetails` | `{ operationId: OperationId; reason: ReasonCode; affectedRelativePaths: BoundedList<SafeRelativePath> }` |
| `InternalDetails` | `{ incidentId: IncidentId }` |

The closed aliases used above are exactly:

```ts
export type RequiredCapability = "write" | "broad" | "gitWrite";
export type WorkspaceLimitScope = "categories" | "guides" | "managedDocumentBytes" | "attachmentCandidates" | "attachmentBytes";
export type GitSearchSourceKind = "bundled" | "system" | "guiPath";
export type GitCredentialKind = "sshAgent" | "credentialHelper" | "unknown";
export type GitExitKind = "spawn" | "exitStatus" | "signal" | "timeout" | "outputLimit";
export type GitConflictKind = "merge" | "rebase" | "cherryPick" | "revert" | "applyMailbox" | "index" | "unknown";
```

The final three declarations use the detail aliases in the exact form below:

```ts
export type ErrorEnvelope = {
  version: 1;
  message: PublicText;
  requestId: RequestId;
  recovery: PublicText;
};
export type ErrorVariant =
  | { code: "INVALID_ARGUMENT"; retryable: false; details: InvalidArgumentDetails }
  | { code: "WORKSPACE_NOT_FOUND"; retryable: true; details: WorkspaceNotFoundDetails }
  | { code: "WORKSPACE_UNAVAILABLE"; retryable: true; details: WorkspaceUnavailableDetails }
  | { code: "WORKSPACE_READ_ONLY"; retryable: false; details: WorkspaceReadOnlyDetails }
  | { code: "WORKSPACE_LIMIT_EXCEEDED"; retryable: false; details: WorkspaceLimitExceededDetails }
  | { code: "SCHEMA_UNSUPPORTED"; retryable: false; details: SchemaUnsupportedDetails }
  | { code: "INVALID_MANIFEST"; retryable: false; details: InvalidManifestDetails }
  | { code: "INVALID_CATEGORY"; retryable: false; details: InvalidCategoryDetails }
  | { code: "INVALID_GUIDE"; retryable: false; details: InvalidGuideDetails }
  | { code: "UNSAFE_ROUND_TRIP"; retryable: false; details: UnsafeRoundTripDetails }
  | { code: "DUPLICATE_ID"; retryable: false; details: DuplicateIdDetails }
  | { code: "CATEGORY_NOT_FOUND"; retryable: false; details: CategoryNotFoundDetails }
  | { code: "GUIDE_NOT_FOUND"; retryable: false; details: GuideNotFoundDetails }
  | { code: "ATTACHMENT_NOT_FOUND"; retryable: false; details: AttachmentNotFoundDetails }
  | { code: "REVISION_CONFLICT"; retryable: false; details: RevisionConflictDetails }
  | { code: "SNAPSHOT_CONFLICT"; retryable: false; details: SnapshotConflictDetails }
  | { code: "PATH_ESCAPE"; retryable: false; details: PathEscapeDetails }
  | { code: "SYMLINK_DENIED"; retryable: false; details: SymlinkDeniedDetails }
  | { code: "HARDLINK_DENIED"; retryable: false; details: HardlinkDeniedDetails }
  | { code: "PERMISSION_DENIED"; retryable: false; details: PermissionDeniedDetails }
  | { code: "FILE_TOO_LARGE"; retryable: false; details: FileTooLargeDetails }
  | { code: "IO_BUSY"; retryable: true; details: IoBusyDetails }
  | { code: "INDEX_NOT_READY"; retryable: true; details: IndexNotReadyDetails }
  | { code: "PLAN_STALE"; retryable: false; details: PlanStaleDetails }
  | { code: "CONFIRMATION_REQUIRED"; retryable: false; details: ConfirmationRequiredDetails }
  | { code: "CONFIRMATION_INVALID"; retryable: false; details: ConfirmationInvalidDetails }
  | { code: "TEST_APPROVAL_REQUIRED"; retryable: false; details: TestApprovalRequiredDetails }
  | { code: "GIT_UNAVAILABLE"; retryable: false; details: GitUnavailableDetails }
  | { code: "GIT_UNSAFE_REPOSITORY"; retryable: false; details: GitUnsafeRepositoryDetails }
  | { code: "GIT_AUTH_REQUIRED"; retryable: true; details: GitAuthRequiredDetails }
  | { code: "GIT_DIVERGED"; retryable: false; details: GitDivergedDetails }
  | { code: "GIT_CONFLICT"; retryable: false; details: GitConflictDetails }
  | { code: "GIT_FAILED"; retryable: false; details: GitFailedDetails }
  | { code: "MIGRATION_REQUIRED"; retryable: false; details: MigrationRequiredDetails }
  | { code: "RECOVERY_REQUIRED"; retryable: false; details: RecoveryRequiredDetails }
  | { code: "INTERNAL"; retryable: false; details: InternalDetails };
export type ErrorDto = ErrorEnvelope & ErrorVariant;
```

No alternate direct-union form, differently named envelope/variant/detail
alias, reordered declaration set, optional common field, `null`, index
signature, or independent `code`/`retryable`/`details` union is accepted.

### Unknown YAML construction API

ADR 0011's semantic representation and work meters are unchanged. Phase 1B
exports exactly this parser-neutral assembly surface from `domain`:

```rust
pub enum UnknownYamlEvent {
    BeginSequence,
    EndSequence,
    BeginMapping,
    EndMapping,
    Null,
    Boolean(bool),
    Integer(CanonicalYamlInteger),
    Float(CanonicalYamlFloat),
    String(String),
}

pub struct UnknownYamlDocumentBuilder { /* private */ }

impl UnknownYamlDocumentBuilder {
    pub fn new() -> Self;
    pub fn push_event(
        &mut self,
        event: UnknownYamlEvent,
    ) -> Result<(), UnknownYamlBuildError>;
    pub fn finish_document(
        self,
    ) -> Result<UnknownMetadataDocument, UnknownYamlBuildError>;
}
```

There are no convenience `begin_*`, `end_*`, `push_*`, `from_iter`, limit-
configuration, unchecked, value-returning, mutable, or raw representation APIs.
`UnknownYamlEvent` owns its payload. `UnknownYamlDocumentBuilder` has no
`Default`, `Clone`, Serde, TS, or reusable reset.

The first event must be `BeginMapping`, which is the one root node at depth 1.
Only a completed `String` scalar may be a root key; attempting a collection or
other scalar as a root key fails immediately. Root values and nested keys may be
any semantic value. The builder preserves entry/sequence encounter order,
requires exactly one root, enforces key/value alternation, and rejects a wrong
or unmatched end event. `finish_document` succeeds only after the root mapping
has ended with no open frame or extra root.

The resulting `UnknownMetadataDocument` represents the complete generic YAML
mapping supplied to the builder. It is not an independently constructible
extension bag. Phase 1D's typed binder must consume or reference exactly one
complete generic root and partition every root entry exactly once into a known
typed field or preserved unknown metadata. No aggregate constructor may accept
caller-supplied known fields alongside a separately constructed
`UnknownMetadataDocument`, because that would bypass duplicate-field and
known/unknown partition checks. Phase 1B does not classify any root key as a
reserved product field.

Every failure first replaces internal state with `Poisoned`, drops all partial
frames/root/canonical-key caches, and returns an error. Later `push_event` or
`finish_document` returns `Poisoned`; no API can extract a partial value. Event
payloads are validated before attachment. Limit equality is accepted and the
next unit/event is refused with no partial result.

The public, non-content-bearing `UnknownYamlBuildErrorKind` variants are exactly:

```text
Poisoned RootMustBeMapping MultipleRoots UnexpectedEnd MismatchedEnd
IncompleteMappingEntry TopLevelKeyMustBeString DuplicateMappingKey
InvalidString DepthLimit MappingEntryLimit SequenceEntryLimit NodeLimit
ScalarLimit NumericDigitLimit SemanticEncodingLimit SemanticComparisonLimit
ArithmeticOverflow IncompleteDocument
```

`UnknownYamlBuildError` exposes only `kind()`, fixed `Display`, and
`std::error::Error`. Every kind shares the exact non-content-bearing Display
text `invalid unknown YAML event sequence`; callers use `kind()` for the closed
reason. Error precedence for one event is: poisoned/finished state;
structural position/end-kind; string/canonical-scalar validity and scalar size;
depth; node count; collection child count; aggregate numeric digits; semantic
encoding/comparison work; duplicate key. Checked arithmetic failure is always
`ArithmeticOverflow` before state mutation.

On `finish_document`, a never-started builder or any open collection with no
pending mapping key returns `IncompleteDocument`; an open mapping whose key has
no value returns `IncompleteMappingEntry`. Both failures poison and destroy the
partial state before returning. A completed root mapping succeeds, while an
extra event after it closed is rejected as `MultipleRoots` during
`push_event`.

`CanonicalYamlInteger::new(negative, digits: String)` is public and fallible;
it requires nonempty ASCII decimal digits, rejects a leading zero except exact
`0`, normalizes negative zero to positive, and rejects more than 65,536 digits.
It exposes `is_negative()` and `digits()`. `CanonicalYamlFloat::finite(negative,
coefficient_digits: String, exponent10: CanonicalYamlInteger)` is public and
fallible; coefficient rules and zero/exponent rules are exactly ADR 0011, and
coefficient plus exponent digits may not exceed 65,536. The public infallible
constructors are `positive_infinity()`, `negative_infinity()`, and `nan()`.
Read-only accessors expose kind/sign/coefficient/exponent without an enum payload
or mutable string. Constructor errors do not retain input.

For direct events, scalar size is UTF-8 bytes for `String`, integer digit bytes,
and coefficient-plus-exponent digit bytes for a finite float; the fixed null,
Boolean, infinity, and NaN cases are necessarily below the limit. Strings
containing a Unicode noncharacter are rejected as `InvalidString`. Every numeric
occurrence charges its integer digits, or its finite coefficient and exponent
digits, toward the exact 80 KiB document limit even if a canonical value was
cloned. Source spelling and ADR 0011's `S`/`R` conversion-work meter do not exist
in 1B; Phase 1C applies those checks before producing these already canonical
events.

The builder applies the exact 16-frame, 20,000-node, 256-entry-per-map,
2,048-entry-per-sequence, 64 KiB scalar, 80 KiB numeric-digit, 8 MiB cached-key-
encoding, and 32 MiB comparison-read limits from ADR 0011 and
`docs/content-model.md`. A node is charged when a start/scalar event is accepted;
a collection entry is charged only when its complete child closes. The root is
one mapping node and depth 1. Canonical key encoding, key-context signed-zero
normalization, bottom-up caching, stable merge ordering, byte-read charging, and
the final adjacent duplicate scan are exactly ADR 0011; using `sort`, a hash map,
or unmetered public equality for duplicate detection is forbidden.

`UnknownMetadataDocument`, `UnknownMetadataEntry`, `UnknownYamlValue`,
`UnknownYamlMappingEntry`, and `UnknownYamlScalar` retain private fields and
private representation enums. They expose only slices/references and kind
discriminants: document `entries`; entry `key`/`value`; value `kind`,
`as_scalar`, `as_sequence`, `as_mapping`; mapping entry `key`/`value`; scalar
`kind`, `as_bool`, `as_integer`, `as_float`, `as_str`. Mismatched `as_*` returns
`None`. No accessor moves a child, returns `&mut`, exposes a cache, or constructs
a value. Cloning a completed valid tree is permitted; no ordinary semantic
`PartialEq`/`Eq` implementation is exposed because repeated unmetered recursive
comparisons would bypass the work policy.

The exact public read API is:

```rust
pub enum UnknownYamlValueKind { Scalar, Sequence, Mapping }
pub enum UnknownYamlScalarKind { Null, Boolean, Integer, Float, String }
pub enum CanonicalYamlFloatKind {
    Finite,
    PositiveInfinity,
    NegativeInfinity,
    NaN,
}

impl UnknownMetadataDocument {
    pub fn entries(&self) -> &[UnknownMetadataEntry];
}
impl UnknownMetadataEntry {
    pub fn key(&self) -> &str;
    pub fn value(&self) -> &UnknownYamlValue;
}
impl UnknownYamlValue {
    pub fn kind(&self) -> UnknownYamlValueKind;
    pub fn as_scalar(&self) -> Option<&UnknownYamlScalar>;
    pub fn as_sequence(&self) -> Option<&[UnknownYamlValue]>;
    pub fn as_mapping(&self) -> Option<&[UnknownYamlMappingEntry]>;
}
impl UnknownYamlMappingEntry {
    pub fn key(&self) -> &UnknownYamlValue;
    pub fn value(&self) -> &UnknownYamlValue;
}
impl UnknownYamlScalar {
    pub fn kind(&self) -> UnknownYamlScalarKind;
    pub fn as_bool(&self) -> Option<bool>;
    pub fn as_integer(&self) -> Option<&CanonicalYamlInteger>;
    pub fn as_float(&self) -> Option<&CanonicalYamlFloat>;
    pub fn as_str(&self) -> Option<&str>;
}
impl CanonicalYamlInteger {
    pub fn is_negative(&self) -> bool;
    pub fn digits(&self) -> &str;
}
impl CanonicalYamlFloat {
    pub fn kind(&self) -> CanonicalYamlFloatKind;
    pub fn is_negative(&self) -> Option<bool>;
    pub fn coefficient_digits(&self) -> Option<&str>;
    pub fn exponent10(&self) -> Option<&CanonicalYamlInteger>;
}
```

For `CanonicalYamlFloat`, `is_negative()` returns `Some(sign)` for finite values
and both infinities, and `None` for NaN. `coefficient_digits()` and
`exponent10()` return `Some` only for finite values and `None` for infinities
and NaN.

## Required Phase 1B evidence

The implementation packet must require all of the following:

- Exact accept/reject vectors for every UUID wrapper and prefix, including
  uppercase/alternate forms, every non-v7 version, invalid variant, and UUIDv7
  payload boundaries; compile evidence that no Rehow ID generator or dependency
  generation API is re-exported, plus source/deny-list evidence that
  `rehow-core` never calls UUID/random generation even when another Workspace
  member causes feature unification.
- Timestamp min/max, every leap-year/month boundary, fraction lengths 0–10,
  trailing-zero rejection, leap-second/offset/year-zero/expanded-year refusal,
  nanosecond ordering, canonical round trip, and Serde string-only fixtures.
- Decimal min/max/canonical/refusal vectors; digest prefix/length/case vectors;
  plan-token RFC 4648 vectors covering every permitted final-character class,
  nonzero pad bits, padding, alphabet, length, exact decode/re-encode, redacted
  Debug, and absence of Display/generation.
- Safe display exact/one-over bounds, all prohibited scalar classes,
  noncharacters, invalid UTF-8 byte escapes, raw/lowercase/noncanonical `%`
  escapes, encoded slash/dot traversal, empty segments, DNS label/host limits,
  canonical IPv4/IPv6, and URL/userinfo/port/zone rejection.
- One golden JSON fixture for each of the 36 error codes; exact seven top-level
  keys; every required/optional detail; omitted-versus-null behavior; all five
  retryable cases and code/details/retryability mismatch rejection; producer
  message/recovery catalog; valid noncatalog inbound copy acceptance and exact
  preservation; invalid `PublicText` rejection; paired source positions; entity
  correlation; unknown detail acceptance/drop on reserialization; unknown
  top-level rejection; duplicate known keys; integer/string boundary types;
  redacted Debug; producer-fixed Git sanitized reasons; and valid noncatalog
  inbound Git reason preservation.
- All bounded-list vectors already named in `docs/testing-strategy.md`, including
  construction and deserialization. No fixture contains an ambient path,
  command, credential, raw parser/Git/OS output, or secret token except the
  intentionally serialized plan-token field required by its code.
- Generated TypeScript golden output, deterministic contract drift, `tsc`
  exhaustive/negative correlation fixtures, and proof that no unknown-YAML type
  is generated.
- Builder success fixtures for every scalar/value kind, nested complex keys,
  encounter order, mapping-order-insensitive duplicate equality, sequence-order
  distinction, integer/float distinction, signed float zero, NaN, and canonical
  numeric accessors. Test every error kind and poisoned follow-up.
- Builder exact-limit and one-over tests for depth, nodes, mapping/sequence
  entries, scalar bytes, aggregate numeric digits, cached encoding bytes, and
  comparison reads; checked-overflow hooks; stable-merge counter vectors; final
  adjacent duplicate detection; and proof that no partial result survives.
- `trybuild` pass/fail fixtures from an external-crate perspective proving that
  callers cannot access representation modules/fields/enums or unchecked
  constructors; construct an entry/value/document directly; deserialize or
  generate TypeScript for unknown YAML; configure/skip limits; extract a partial
  value; construct emitting text/token catalogs; construct details/ErrorDto; or
  call UUID/token/timestamp/digest generation. Positive compile fixtures prove
  the documented parse, accessor, builder, Serde, and contract APIs.
- An isolated `cargo tree -e features -p rehow-core` records the direct core
  graph and verifies no core-selected RNG/getrandom, unsafe Base64 SIMD,
  local-time, YAML, BLAKE3, or accidental dev dependency in its normal graph.
  A second full-Workspace feature tree records existing feature unification
  (including Tauri's UUID generation and any extra `time` features) rather than
  falsely claiming those dependency APIs are globally absent. Review proves by
  core source/compile-fail checks that no generation call or re-export crosses
  the Rehow core API. Review all new lockfile packages and record before/after
  release artifact sizes.

Run `pnpm contracts:generate`, `pnpm contracts:check`, `pnpm check`, `pnpm test`,
`pnpm test:integration`, `pnpm build`, `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace --all-features --locked`, the bootstrap check, and
`git diff --check`. Tests use no Workspace, filesystem path, Git repository,
clock, randomness, network, or user content.

## Explicit non-goals

Phase 1B does not generate any ID, timestamp, digest, revision, snapshot,
verification hash, or plan token. It does not add clock/entropy traits or
implement plan storage, expiry, single use, binding, confirmation, approval,
request middleware, incident reporting, or logging. Lexical validity conveys no
authority.

It does not define or construct Workspace Manifest, Category, Guide, Reference,
Attachment, status-transition, verification-content-hash, or application use-
case aggregates. It does not parse, resolve, emit, serialize, or edit YAML or
Markdown; add ADR 0011 parser dependencies; expose unknown YAML over Serde/TS;
touch filesystem/storage/confinement/recovery; hash content; implement Git/MCP/
Tauri commands; sanitize raw external text; or convert OS paths/Git output/URLs
to public display types. It does not modify adapters, UI, capabilities, CSP,
logs, scripts, toolchains, package manifests, or JavaScript dependencies.
It does not alter project agent configuration: every project agent remains
`gpt-5.6-sol` at the role effort fixed by `AGENTS.md`.

Phase 1C owns bounded YAML source conversion into the event API. Phase 1D owns
product aggregate validation and typed edit authority. Phase 1E owns BLAKE3
revision/snapshot calculation, confinement, and storage/recovery. Later status,
Git, and MCP decisions own their respective generation and sanitization gates.

## Consequences

- Phase 1B can prove the complete public error and primitive wire boundary
  without prematurely granting time, randomness, filesystem, hashing, Git, or
  parser authority.
- Strict canonical values make revisions, plans, fixtures, and cross-adapter
  comparisons deterministic, while fixed catalogs prevent external text from
  leaking through errors.
- Custom Serde and a correlated TypeScript union cost more code than derives,
  but make invalid code/detail/retryability states unrepresentable at the core
  boundary.
- The event builder is deliberately less convenient than public value
  constructors; that inconvenience is the non-bypassable resource and duplicate
  key boundary required by ADR 0011.
- Actual ID/token/time generation remains blocked until its owning phase reviews
  fallibility, monotonicity, entropy failure, persistence, and authority.

## Rejected alternatives

- Generate UUIDv7/request IDs or plan tokens in 1B: this would introduce ambient
  clock/entropy and lifetime semantics before an owning application service.
- Enable `uuid` v7/Serde/RNG features or Base64 defaults: unnecessary dependency
  and unsafe/authority surface.
- Accept general RFC 3339 offsets, leap seconds, arbitrary fractional spelling,
  or local time: multiple spellings and comparison edge cases undermine a
  canonical portable form.
- Derive Serde directly for opaque primitives or use an open error details map:
  permissive spellings and invalid code/detail combinations become constructible.
- Public constructors for reason/path/message tokens: validated syntax alone does
  not establish safe provenance.
- Public constructors/Serde for unknown YAML nodes or configurable builder
  limits: either bypasses the bounded semantic construction boundary.
- Use JSON values, `f64`, machine integers, recursive all-pairs equality, or a
  hash map for unknown YAML: these violate ADR 0011 semantics or deterministic
  work accounting.

## Sources

- [`uuid` 1.26.0 documentation and features](https://docs.rs/uuid/1.26.0/uuid/)
- [`time` 0.3.55 documentation and features](https://docs.rs/time/0.3.55/time/)
- [RUSTSEC-2026-0009, patched in `time >=0.3.47`](https://rustsec.org/advisories/RUSTSEC-2026-0009.html)
- [`base64` 0.23.1 documentation](https://docs.rs/base64/0.23.1/base64/)
- [`serde_json` 1.0.151](https://docs.rs/serde_json/1.0.151/serde_json/)
- [`trybuild` 1.0.120](https://docs.rs/trybuild/1.0.120/trybuild/)
- [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339)
- [RFC 9562 UUID version 7](https://www.rfc-editor.org/rfc/rfc9562#name-uuid-version-7)
- [RFC 4648 Base64url canonical encoding](https://www.rfc-editor.org/rfc/rfc4648#section-5)
