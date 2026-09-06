import type * as Contracts from "./generated";
import type { ErrorDto, InvalidArgumentDetails, U64Decimal } from "./generated";

// @ts-expect-error unknown YAML never crosses the protocol boundary
type UnknownYamlValue = Contracts.UnknownYamlValue;

export function exhaust(error: ErrorDto): string {
  switch (error.code) {
    case "INVALID_ARGUMENT":
      return error.details.field;
    case "WORKSPACE_NOT_FOUND":
      return error.details.recentWorkspaceId;
    case "WORKSPACE_UNAVAILABLE":
      return error.details.reason;
    case "WORKSPACE_READ_ONLY":
      return error.details.reason;
    case "WORKSPACE_LIMIT_EXCEEDED":
      return error.details.limit;
    case "SCHEMA_UNSUPPORTED":
      return String(error.details.foundVersion);
    case "INVALID_MANIFEST":
      return error.details.relativePath;
    case "INVALID_CATEGORY":
      return error.details.relativePath;
    case "INVALID_GUIDE":
      return error.details.relativePath;
    case "UNSAFE_ROUND_TRIP":
      return error.details.relativePath;
    case "DUPLICATE_ID":
      return error.details.entityId;
    case "CATEGORY_NOT_FOUND":
      return error.details.categoryId;
    case "GUIDE_NOT_FOUND":
      return error.details.guideId;
    case "ATTACHMENT_NOT_FOUND":
      return error.details.attachmentId;
    case "REVISION_CONFLICT":
      return error.details.expectedRevision;
    case "SNAPSHOT_CONFLICT":
      return error.details.expectedSnapshot;
    case "PATH_ESCAPE":
      return error.details.reason;
    case "SYMLINK_DENIED":
      return error.details.relativePath;
    case "HARDLINK_DENIED":
      return error.details.linkCount;
    case "PERMISSION_DENIED":
      return error.details.operation;
    case "FILE_TOO_LARGE":
      return error.details.limitBytes;
    case "IO_BUSY":
      return String(error.details.timeoutMs);
    case "INDEX_NOT_READY":
      return error.details.indexGeneration;
    case "PLAN_STALE":
      return error.details.operation;
    case "CONFIRMATION_REQUIRED":
      return error.details.planToken;
    case "CONFIRMATION_INVALID":
      return error.details.reason;
    case "TEST_APPROVAL_REQUIRED":
      return error.details.guideId;
    case "GIT_UNAVAILABLE":
      return String(error.details.searchSourceKinds.totalCount);
    case "GIT_UNSAFE_REPOSITORY":
      return error.details.reason;
    case "GIT_AUTH_REQUIRED":
      return error.details.host;
    case "GIT_DIVERGED":
      return error.details.ahead;
    case "GIT_CONFLICT":
      return error.details.conflictKind;
    case "GIT_FAILED":
      return error.details.sanitizedReason;
    case "MIGRATION_REQUIRED":
      return String(error.details.targetVersion);
    case "RECOVERY_REQUIRED":
      return error.details.operationId;
    case "INTERNAL":
      return error.details.incidentId;
    default: {
      const neverError: never = error;
      return neverError;
    }
  }
}
const details: InvalidArgumentDetails = { field: "/title", reason: "required" };
const decimal: U64Decimal = "18446744073709551615";
void details;
void decimal;
const badLocation: Extract<ErrorDto, { code: "INVALID_MANIFEST" }> = {
  version: 1,
  code: "INVALID_MANIFEST",
  message: "x",
  requestId: "x",
  recovery: "x",
  retryable: false,
  // @ts-expect-error source locations require line and column together
  details: {
    relativePath: "workspace.yml",
    diagnosticCode: "invalid_field_value",
    line: 1,
  },
};
// @ts-expect-error wire decimals are strings
const badDecimal: U64Decimal = 1;
const badOptional: Extract<ErrorDto, { code: "PATH_ESCAPE" }> = {
  version: 1,
  code: "PATH_ESCAPE",
  message: "x",
  requestId: "x",
  recovery: "x",
  retryable: false,
  // @ts-expect-error nullable optional values are forbidden
  details: { reason: "x", relativePath: null },
};
// @ts-expect-error retryability is correlated
const badRetry: ErrorDto = {
  version: 1,
  code: "IO_BUSY",
  message: "x",
  requestId: "x",
  recovery: "x",
  retryable: false,
  details: { operation: "x", timeoutMs: 1 },
};
const badDetails: ErrorDto = {
  version: 1,
  code: "CATEGORY_NOT_FOUND",
  message: "x",
  requestId: "x",
  recovery: "x",
  retryable: false,
  // @ts-expect-error code and details are correlated
  details: { guideId: "x" },
};
void badDecimal;
void badLocation;
void badOptional;
void badRetry;
void badDetails;
void (undefined as unknown as UnknownYamlValue);
