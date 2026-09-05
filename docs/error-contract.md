# Public Error Contract

Status: protocol v1 baseline
Owner: `rehow_core::protocol`
Last updated: 2026-09-05

## Shape

Tauri and MCP expose the same Rust-owned `ErrorDto`:

```json
{
  "version": 1,
  "code": "REVISION_CONFLICT",
  "message": "The Guide changed since it was opened.",
  "retryable": false,
  "requestId": "0199…",
  "details": {
    "entityKind": "guide",
    "entityId": "0199…",
    "expectedRevision": "rev:v1:…",
    "actualRevision": "rev:v1:…"
  },
  "recovery": "Read the Guide again, compare changes, and retry with its new revision."
}
```

`code`, `retryable`, and documented detail keys are stable in protocol major v1. Human `message`/`recovery` text may improve without a protocol break. Unknown detail keys must be ignored by clients. Absolute paths, raw content, command lines, arbitrary Git stderr, environment values, URLs with credentials/query secrets, and recovery-object locations are forbidden.

## Codes and allowed details

| Code | Retryable | Allowed detail keys |
| --- | --- | --- |
| `INVALID_ARGUMENT` | no | `field`, `reason` |
| `WORKSPACE_NOT_FOUND` | yes | `recentWorkspaceId` |
| `WORKSPACE_UNAVAILABLE` | yes | `reason` |
| `WORKSPACE_READ_ONLY` | no | `reason`, `requiredCapability` |
| `WORKSPACE_LIMIT_EXCEEDED` | no | `limit`, `observedAtLeast`, `scope` |
| `SCHEMA_UNSUPPORTED` | no | `foundVersion`, `minimumVersion`, `maximumVersion` |
| `INVALID_MANIFEST` | no | `relativePath`, `field`, `diagnosticCode`, `line`, `column` |
| `INVALID_CATEGORY` | no | same diagnostic keys plus `categoryId` |
| `INVALID_GUIDE` | no | same diagnostic keys plus `guideId` |
| `UNSAFE_ROUND_TRIP` | no | `relativePath`, `field`, `reason` |
| `DUPLICATE_ID` | no | `entityKind`, `entityId`, `relativePaths` |
| `CATEGORY_NOT_FOUND` | no | `categoryId` |
| `GUIDE_NOT_FOUND` | no | `guideId` |
| `ATTACHMENT_NOT_FOUND` | no | `attachmentId`, `guideId` |
| `REVISION_CONFLICT` | no | `entityKind`, `entityId`, `expectedRevision`, `actualRevision` |
| `SNAPSHOT_CONFLICT` | no | `expectedSnapshot`, `actualSnapshot`, `changedEntityIds` |
| `PATH_ESCAPE` | no | `relativePath`, `reason` |
| `SYMLINK_DENIED` | no | `relativePath` |
| `HARDLINK_DENIED` | no | `relativePath`, `linkCount` |
| `PERMISSION_DENIED` | no | `operation`, `relativePath` |
| `FILE_TOO_LARGE` | no | `relativePath`, `limitBytes`, `observedAtLeastBytes` |
| `IO_BUSY` | yes | `operation`, `timeoutMs` |
| `INDEX_NOT_READY` | yes | `indexGeneration`, `progressCompleted`, `progressTotal` |
| `PLAN_STALE` | no | `operation`, `changedEntityIds` |
| `CONFIRMATION_REQUIRED` | no | `operation`, `planToken`, `expiresAt` |
| `CONFIRMATION_INVALID` | no | `operation`, `reason` |
| `TEST_APPROVAL_REQUIRED` | no | `guideId`, `guideRevision`, `approvalRequestId` |
| `GIT_UNAVAILABLE` | no | `searchSourceKinds` (`bundled`, `system`, or `guiPath`; never absolute paths) |
| `GIT_UNSAFE_REPOSITORY` | no | `reason` |
| `GIT_AUTH_REQUIRED` | yes | `remoteName`, `host`, `credentialKind` |
| `GIT_DIVERGED` | no | `ahead`, `behind`, `branch` |
| `GIT_CONFLICT` | no | `conflictKind`, `relativePaths` |
| `GIT_FAILED` | no | `operation`, `exitKind`, `sanitizedReason` |
| `MIGRATION_REQUIRED` | no | `foundVersion`, `targetVersion` |
| `RECOVERY_REQUIRED` | no | `operationId`, `reason`, `affectedRelativePaths` |
| `INTERNAL` | no | `incidentId` |

Collections in details are bounded/truncated with an explicit count. `relativePath` is displayed escaped and only after confinement; MCP may replace it with an entity ID when disclosure is unnecessary.

## Adapter mapping

- Core/application returns `Result<T, ErrorDto>`; adapters do not rename codes or change retryability.
- Tauri rejects malformed IPC before application dispatch as `INVALID_ARGUMENT`; valid domain failures return `ErrorDto` unchanged.
- MCP uses protocol errors only for invalid JSON-RPC/tool-schema/transport conditions. A valid tool invocation with a domain failure returns `isError: true` and this exact object in structured content.
- Logs use `requestId`/`incidentId` and code, with the same redaction rules.
- UI branches on `code`, not English text. Snapshot tests freeze serialization and adapter parity.
