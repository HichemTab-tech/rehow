# Public Error Contract

Status: protocol v1 baseline
Owner: `rehow_core::protocol`
Last updated: 2026-09-06

## Shape and construction

Tauri and MCP expose the same Rust-owned `ErrorDto`:

```json
{
  "version": 1,
  "code": "REVISION_CONFLICT",
  "message": "The item changed since it was opened.",
  "retryable": false,
  "requestId": "0199a1d7-7b0d-7d8e-a38d-f936c5735e10",
  "details": {
    "entityKind": "guide",
    "entityId": "0199a1d7-7b0d-7d8e-a38d-f936c5735e10",
    "expectedRevision": "rev:v1:0000000000000000000000000000000000000000000000000000000000000000",
    "actualRevision": "rev:v1:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
  },
  "recovery": "Read it again, compare changes, and retry with its new revision."
}
```

All seven top-level keys are required and non-null. `version` is the JSON integer literal `1`. The core represents code/details as one exhaustive discriminated Rust enum; a caller cannot construct an arbitrary code/details pairing or choose retryability. A single exhaustive match derives `retryable`; deserialization rejects a mismatch. Optional detail properties are omitted, never serialized as `null`. Consumers ignore unknown detail properties for forward compatibility, but producers never emit undocumented properties.

`code`, its correlated detail shape, and retryability are stable in protocol major v1. Human `message` and `recovery` text may improve without a protocol break; both remain nonempty core-owned text. Every public field, including `details`, is subject to one global disclosure rule: never include raw file/Guide content, command lines, Git stderr/stdout, ambient absolute paths, environment values, recovery-object locations, credentials/tokens, or unredacted credential-bearing URLs. The only path/name exceptions are the separately constructed bounded public-display types below. Optional details are omitted whenever their safe provenance cannot be proved.

## Wire primitives

- ID fields are lowercase canonical UUIDv7 strings with distinct Workspace, Category, Guide, Attachment, request, approval, operation, and incident Rust newtypes.
- `PortableEntityId` is `{ "kind": "workspace" | "category" | "guide" | "attachment", "id": <the matching typed UUIDv7> }`; mismatched kind/ID variants cannot be constructed.
- `Revision` is `rev:v1:` plus exactly 64 lowercase hexadecimal BLAKE3 characters; `Snapshot` is `snapshot:v1:` plus exactly 64 lowercase hexadecimal BLAKE3 characters; and `VerificationHash` is `verify:v1:` plus exactly 64 lowercase hexadecimal BLAKE3 characters.
- `PlanToken` is `plan:v1:` plus the canonical unpadded URL-safe Base64 encoding of exactly 32 random process-local bytes. The payload has exactly 43 characters matching `[A-Za-z0-9_-]{43}`; because a 32-byte payload leaves two padding bits, its final character must be one of `[AEIMQUYcgkosw048]`. Validation decodes exactly 32 bytes and requires re-encoding to reproduce the original 43 characters byte-for-byte, rejecting nonzero padding bits and alternate encodings. It is single-use and resolves server-side to the bound plan/effect set; its text never contains that state. Phase 1B is parse-only; generation, entropy, binding, and single-use enforcement are deferred to the owning approval/plan service. `RecentWorkspaceId` is `recent:v1:` plus one lowercase canonical UUIDv7 for a native-preference record; its generation is likewise deferred to the owning native-preference phase. Callers compare/pass these opaque identifiers but do not construct authority from them.
- `U64Decimal` is a quoted canonical unsigned decimal string matching `0|[1-9][0-9]*` and bounded to `u64`. JSON numbers are rejected for these fields.
- `SignedInt64Decimal` is a quoted canonical signed decimal string matching the full grammar `0|-?[1-9][0-9]*`, bounded to `i64`; `-0`, a leading `+`, whitespace, and leading zeroes are invalid.
- `u32` means a JSON integer from `0` through `4294967295`.
- `SchemaFieldPath` is a core-owned JSON Pointer naming a documented schema field. It is never copied from an untrusted unknown key. ADR 0014 adds closed indexed templates for Guide collection items and their documented child members. Indices are bounded canonical decimal strings without leading zeroes; an arbitrary unknown key can never become a path.
- `ReasonCode` and `DiagnosticCode` are lowercase ASCII snake-case tokens of 1–64 bytes, not raw parser/Git/OS messages.
- `SafeRelativePath` is a display-only value separately constructed by the core from an already-confined internal relative path; it is never used as filesystem authority. `/` is the only displayed segment separator. C0 (`U+0000..U+001F`), DEL/C1 (`U+007F..U+009F`), `U+061C`, `U+200E`, `U+200F`, `U+2028..U+202E`, `U+2066..U+2069`, `U+FEFF`, Unicode noncharacters, invalid UTF-8 bytes, and literal `%` are rendered as uppercase `%HH` sequences over their UTF-8/raw bytes. The 1,024-byte internal managed-path cap therefore produces at most 3,072 public Unicode scalar values without truncation. A wire validator accepts only nonempty relative slash-separated display paths within that bound, no empty/`.`/`..` segment, no raw prohibited scalar, and only complete uppercase `%HH` escapes. It is safe before either Tauri or MCP serialization; each consumer still renders it as text, never markup. It is never an ambient absolute path.
- `PublicText` is nonempty core-owned text of at most 256 Unicode scalar values. It rejects C0/C1, DEL, `U+061C`, `U+200E`, `U+200F`, `U+2028..U+202E`, `U+2066..U+2069`, `U+FEFF`, and Unicode noncharacters. Phase 1B producers can construct it only from the fixed message/recovery/reason catalog, never raw or interpolated external input. The private `ErrorDto` consumer path may validate and preserve different safe text from a compatible producer because human copy may improve within protocol v1; that path grants no producer construction authority. A later sanitizer requires its own reviewed constructor and adversarial tests before converting Git/OS/parser text to `PublicText`.
- `Timestamp` is an RFC 3339 UTC string written with `Z`.
- `SafeGitDisplay` is nonempty display text derived only from one parsed Git remote-name or symbolic-ref field—not a remote URL, command, arbitrary stdout/stderr blob, or URL-bearing config value. Its wire form is at most 768 Unicode scalar values, uses the same exact prohibited-scalar/invalid-byte/`%` encoding as `SafeRelativePath`, and contains no `/` structural meaning. `SafeHost` is exactly one of: `unknown`; a lowercase ASCII DNS name of at most 253 bytes whose 1–63-byte labels start/end alphanumeric and otherwise contain only alphanumeric/hyphen; canonical dotted-decimal IPv4 with four 0–255 components and no leading zero; or lowercase RFC 5952 IPv6 without brackets or zone identifier. Before DNS fallback, an entire candidate whose dot-separated components each match ASCII `[0-9]+` or `0x[0-9a-f]+` is reserved as IPv4-shaped: only canonical dotted-decimal IPv4 is accepted. This rejects legacy decimal, octal, hexadecimal, and shortened IPv4 aliases such as `2130706433`, `017700000001`, `0x7f000001`, and `127.1`. Numeric-only DNS names are deferred until Git host extraction and resolver semantics are frozen. `SafeHost` contains no userinfo, port, query, or fragment. Neither display value is passed back to a subprocess.
- Phase 1B implements validation/serialization only for already-constructed `SafeRelativePath`, `SafeGitDisplay`, and `SafeHost` wire values. It has no raw filesystem or Git constructor. The confinement phase must freeze and test raw-path-to-display conversion before emitting a path; the Git phase must freeze exact argument-vector output parsing, URL/credential redaction, host extraction, and raw-name-to-display conversion before constructing any Git error. Until those gates pass, the corresponding application error constructors remain crate-private/unreachable outside protocol fixtures.
- `OperationCode` and `GitOperationCode` are lowercase ASCII snake-case protocol tokens of 1–64 bytes (for example `guide_delete`), not arbitrary subprocess input. `GitConflictKind` is the exact closed enum `merge | rebase | cherryPick | revert | applyMailbox | index | unknown`. Other discriminant values use the exact lower-camel values named by their field. Feature-specific accepted operation values are frozen when their phase is accepted; consumers tolerate an unknown operation token for forward-compatible display.

### Bounded lists

Every error-detail collection uses:

```json
{
  "items": ["…"],
  "totalCount": 47,
  "truncated": true
}
```

`BoundedList<T>` contains the deterministic retained prefix, at most 32 items, and an exact `u32` `totalCount`. Its retained length is exactly `min(totalCount, 32)`: producers may not omit known items inside that bound. `truncated` is `false` exactly when `totalCount == items.length`; it is `true` exactly when `totalCount > items.length`. Item order is deterministic for the error code. If a safe exact total is unavailable because scanning stopped at a resource limit, the core returns `WORKSPACE_LIMIT_EXCEEDED` with `observedAtLeast` instead of fabricating a list count. Constructors reject inconsistent count/flag combinations or a shorter-than-available retained prefix.

## Codes and exact details

“Optional” means the property is omitted when unavailable or unsafe to disclose. Every property not marked optional is required.

| Code | Retryable | Required details | Optional details |
| --- | --- | --- | --- |
| `INVALID_ARGUMENT` | no | `field: SchemaFieldPath`, `reason: ReasonCode` | — |
| `WORKSPACE_NOT_FOUND` | yes | `recentWorkspaceId: RecentWorkspaceId` | — |
| `WORKSPACE_UNAVAILABLE` | yes | `reason: ReasonCode` | — |
| `WORKSPACE_READ_ONLY` | no | `reason: ReasonCode` | `requiredCapability: write \| broad \| gitWrite` |
| `WORKSPACE_LIMIT_EXCEEDED` | no | `limit: U64Decimal`, `observedAtLeast: U64Decimal`, `scope: categories \| guides \| managedDocumentBytes \| attachmentCandidates \| attachmentBytes` | — |
| `SCHEMA_UNSUPPORTED` | no | `foundVersion: u32`, `minimumVersion: u32`, `maximumVersion: u32` | — |
| `INVALID_MANIFEST` | no | `relativePath: SafeRelativePath`, `diagnosticCode: DiagnosticCode` | `field: SchemaFieldPath`; `line: u32` and `column: u32` occur together and are one-based |
| `INVALID_CATEGORY` | no | `relativePath: SafeRelativePath`, `diagnosticCode: DiagnosticCode` | `field: SchemaFieldPath`; paired one-based `line: u32`/`column: u32`; `categoryId: CategoryId` only when it parsed |
| `INVALID_GUIDE` | no | `relativePath: SafeRelativePath`, `diagnosticCode: DiagnosticCode` | `field: SchemaFieldPath`; paired one-based `line: u32`/`column: u32`; `guideId: GuideId` only when it parsed |
| `UNSAFE_ROUND_TRIP` | no | `relativePath: SafeRelativePath`, `reason: ReasonCode` | `field: SchemaFieldPath` |
| `DUPLICATE_ID` | no | correlated `entityKind: workspace \| category \| guide \| attachment`, typed `entityId`, `relativePaths: BoundedList<SafeRelativePath>` | — |
| `CATEGORY_NOT_FOUND` | no | `categoryId: CategoryId` | — |
| `GUIDE_NOT_FOUND` | no | `guideId: GuideId` | — |
| `ATTACHMENT_NOT_FOUND` | no | `attachmentId: AttachmentId`, `guideId: GuideId` | — |
| `REVISION_CONFLICT` | no | correlated `entityKind`/typed `entityId`, `expectedRevision: Revision`, `actualRevision: Revision` | — |
| `SNAPSHOT_CONFLICT` | no | `expectedSnapshot: Snapshot`, `actualSnapshot: Snapshot`, `changedEntityIds: BoundedList<PortableEntityId>` | — |
| `PATH_ESCAPE` | no | `reason: ReasonCode` | `relativePath: SafeRelativePath` only if independently confined before escape detection |
| `SYMLINK_DENIED` | no | `relativePath: SafeRelativePath` | — |
| `HARDLINK_DENIED` | no | `relativePath: SafeRelativePath`, `linkCount: U64Decimal` | — |
| `PERMISSION_DENIED` | no | `operation: OperationCode` | `relativePath: SafeRelativePath` |
| `FILE_TOO_LARGE` | no | `relativePath: SafeRelativePath`, `limitBytes: U64Decimal`, `observedAtLeastBytes: U64Decimal` | — |
| `IO_BUSY` | yes | `operation: OperationCode`, `timeoutMs: u32` | — |
| `INDEX_NOT_READY` | yes | `indexGeneration: U64Decimal`, `progressCompleted: u32` | `progressTotal: u32` |
| `PLAN_STALE` | no | `operation: OperationCode`, `changedEntityIds: BoundedList<PortableEntityId>` | — |
| `CONFIRMATION_REQUIRED` | no | `operation: OperationCode`, `planToken: PlanToken`, `expiresAt: Timestamp` | — |
| `CONFIRMATION_INVALID` | no | `operation: OperationCode`, `reason: ReasonCode` | — |
| `TEST_APPROVAL_REQUIRED` | no | `guideId: GuideId`, `guideRevision: Revision`, `approvalRequestId: ApprovalRequestId` | — |
| `GIT_UNAVAILABLE` | no | `searchSourceKinds: BoundedList<bundled \| system \| guiPath>` | — |
| `GIT_UNSAFE_REPOSITORY` | no | `reason: ReasonCode` | — |
| `GIT_AUTH_REQUIRED` | yes | `remoteName: SafeGitDisplay`, `host: SafeHost`, `credentialKind: sshAgent \| credentialHelper \| unknown` | — |
| `GIT_DIVERGED` | no | `ahead: U64Decimal`, `behind: U64Decimal`, `branch: SafeGitDisplay` | — |
| `GIT_CONFLICT` | no | `conflictKind: GitConflictKind`, `relativePaths: BoundedList<SafeRelativePath>` | — |
| `GIT_FAILED` | no | `operation: GitOperationCode`, `exitKind: spawn \| exitStatus \| signal \| timeout \| outputLimit`, `sanitizedReason: PublicText` | — |
| `MIGRATION_REQUIRED` | no | `foundVersion: u32`, `targetVersion: u32` | — |
| `RECOVERY_REQUIRED` | no | `operationId: OperationId`, `reason: ReasonCode`, `affectedRelativePaths: BoundedList<SafeRelativePath>` | — |
| `INTERNAL` | no | `incidentId: IncidentId` | — |

Malformed-document line/column properties occur together. Correlated entity kind/ID pairs are their own discriminated internal enum so a Category kind cannot carry a Guide ID. IDs and safe paths are emitted only after that individual value passed validation. Detail structs have private fields and validated constructors; collection and text bounds apply before serialization.

Phase 1D binding failures are private and content-free. They deterministically identify only a document kind, closed diagnostic kind, closed known-field path, optional parser-proven position, and optional independently validated identity. Phase 1E/application code adds a confined `SafeRelativePath` and maps them to `INVALID_MANIFEST`, `INVALID_CATEGORY`, `INVALID_GUIDE`, `SCHEMA_UNSUPPORTED`, or `UNSAFE_ROUND_TRIP`; adapters do not translate raw parser/dependency text.

## Storage versus protocol types

Portable YAML/domain records and JSON protocol DTOs are distinct mapped types. In particular, Category `order` is an `i64` YAML integer but a `SignedInt64Decimal` JSON string. Do not use serializer `is_human_readable` to switch representation because both JSON and YAML serializers report human-readable. Generated TypeScript represents decimal newtypes as opaque string aliases; validation and conversion remain Rust-owned in v1, and TypeScript callers only display/pass the value. JavaScript `bigint` is not a JSON wire value.

## Adapter mapping

- Core/application returns `Result<T, ErrorDto>`; adapters do not rename codes, change retryability, replace details, or add ambient information.
- The core decides whether an optional safe path is present before the adapter boundary. MCP never substitutes an entity ID for a path inside an existing error.
- Tauri rejects malformed IPC before application dispatch as `INVALID_ARGUMENT`; valid domain failures return `ErrorDto` unchanged.
- MCP uses protocol errors only for invalid JSON-RPC/tool-schema/transport conditions. A valid tool invocation with a domain failure returns `isError: true` and this exact object in structured content.
- Logs contain only `requestId`/`incidentId`, code, and fixed telemetry labels; they never serialize `ErrorDto.details`, `message`, `recovery`, or a Debug representation of the error.
- UI branches on `code`, not English text. Fixed JSON fixtures cover every code, required-field rejection, optional omission, retryability mismatch, redaction, list invariants, and Tauri/MCP byte/schema parity.
