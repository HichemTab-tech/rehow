// Native payload types, runtime validators, and pure presentation helpers.
// Every value crossing the Tauri boundary is validated before it reaches UI state.
import type { SafeMarkdownDocument } from "@rehow/contracts";

/** "unadopted" Guides are hand-written files Rehow lists but hasn't written to yet. */
export type GuideOrigin = "managed" | "unadopted";

export type GuideSummary = {
  id: string;
  origin: GuideOrigin;
  title: string;
  summary: string;
  categoryId: string;
  category: string;
  status: "Tested" | "Draft" | "Stale" | "Archived";
  updatedAt: string;
  tags: string[];
  aliases: string[];
  attachments: Array<{
    id: string;
    title: string;
    sizeBytes: number | null;
  }>;
  matches: string;
};

export type GuideBodyPayload = {
  id: string;
  origin: GuideOrigin;
  title: string;
  summary: string;
  categoryId: string;
  tags: string[];
  aliases: string[];
  revision: string;
  body: string;
  document: SafeMarkdownDocument;
  createdAt: string;
  lastVerifiedAt: string | null;
  appliesTo: Array<{
    platform: string;
    versions: string | null;
    notes: string | null;
  }>;
  prerequisites: string[];
  warnings: string[];
  filesAffected: string[];
  relatedGuideIds: string[];
  references: Array<{ title: string; url: string }>;
};

export type AttachmentTextPreviewPayload = {
  title: string;
  text: string;
  sizeBytes: number;
  sanitized: boolean;
};

export type AttachmentExportPayload =
  | { kind: "cancelled" }
  | { kind: "exported"; sizeBytes: number };

export type AttachmentRevealPayload = { revealed: true };

export type ExternalLinkOpenPayload = { requested: true };

export type FolderFacts = {
  writable: boolean;
  existingGitRepository: boolean;
  nestedGitRepository: boolean;
  gitProbeComplete: boolean;
  unsafeGitMarker: boolean;
};

export type WorkspaceIssueKind =
  | "unsafeEntry"
  | "unsupportedEntry"
  | "missingCategoryDescriptor"
  | "malformedCategory"
  | "malformedGuide"
  | "fileTooLarge"
  | "categoryMismatch"
  | "duplicateCategoryId"
  | "duplicateGuideId"
  | "verificationMismatch"
  | "missingAttachment"
  | "orphanAttachment"
  | "attachmentHashMismatch"
  | "attachmentSizeMismatch";

export type WorkspaceIssuePayload = {
  kind: WorkspaceIssueKind;
  relativePath: string;
  /** Why a Guide file couldn't be read (camelCase core diagnostic code). */
  diagnostic: string | null;
  /** The frontmatter key involved, when known. */
  field: string | null;
  line: number | null;
  column: number | null;
};

export type WorkspacePayload = {
  id: string;
  name: string;
  categories: Array<{
    id: string;
    name: string;
    summary: string | null;
    status: "active" | "archived";
    revision: string;
    guideCount: number;
  }>;
  guides: GuideSummary[];
  issueCount: number;
  issues: WorkspaceIssuePayload[];
  issuesTruncated: boolean;
  facts: FolderFacts;
  watchStatus: "active" | "degraded";
  watchSession: string;
};

export type DesktopPreferencesPayload = {
  syncOnStart: boolean;
  syncIntervalMinutes: number | null;
};

export type RecentWorkspaceResult =
  | { kind: "none" }
  | { kind: "restored"; workspace: WorkspacePayload }
  | { kind: "unavailable"; reason: string };

export const SYNC_INTERVALS = [15, 30, 60, 240, 1_440] as const;

export const WORKSPACE_ISSUE_KINDS: readonly WorkspaceIssueKind[] = [
  "unsafeEntry",
  "unsupportedEntry",
  "missingCategoryDescriptor",
  "malformedCategory",
  "malformedGuide",
  "fileTooLarge",
  "categoryMismatch",
  "duplicateCategoryId",
  "duplicateGuideId",
  "verificationMismatch",
  "missingAttachment",
  "orphanAttachment",
  "attachmentHashMismatch",
  "attachmentSizeMismatch",
];

export type GitStatusPayload = {
  kind:
    | "unavailable"
    | "notRepository"
    | "unsafe"
    | "ready"
    | "failed"
    | "recoveryRequired";
  version: string | null;
  branch: string | null;
  detached: boolean;
  upstream: string | null;
  divergenceKnown: boolean;
  ahead: number;
  behind: number;
  changed: number;
  untracked: number;
  conflicts: number;
  remotes: string[];
  remoteCount: number;
  identity: "configured" | "missing" | "invalid" | null;
  globalIdentity: "available" | "notConfigured" | "unsafe";
  failure: string | null;
};

export type GitInitPlanPayload = {
  planToken: string;
  initialBranch: "main";
};

export type GitInitResultPayload = {
  initialized: boolean;
};

export type GitRemotePlanPayload = {
  planToken: string;
  remoteName: string;
  transport: "https" | "ssh";
  host: string;
};

export type GitRemoteResultPayload = {
  connected: boolean;
};

export type GitIdentityPlanPayload = {
  planToken: string;
  source: "manual" | "global";
};

export type GitIdentityResultPayload = {
  configured: boolean;
};

export type GitCommitPlanPayload = {
  planToken: string;
  branch: string;
  created: number;
  updated: number;
  deleted: number;
  unrelatedChanges: number;
};

export type GitCommitResultPayload = {
  committed: boolean;
  workspaceChanged: boolean;
};

export type GitCommitCancelResultPayload = {
  cancelled: boolean;
};

export type GitCommitRecoveryResultPayload = {
  recovered: boolean;
  workspaceChanged: boolean;
};

export type GitSyncAncestry =
  | "upToDate"
  | "localAhead"
  | "remoteFastForward"
  | "diverged"
  | "historyLimit";

export type GitSyncInspectionPayload = {
  planToken: string;
  remoteName: string;
  transport: "https";
  host: string;
  ancestry: GitSyncAncestry;
  localChanges: boolean;
  workspaceMatchesRemote: boolean;
};

export type GitSyncCancelResultPayload = {
  cancellationRequested: boolean;
};

export type GitSyncApplyResultPayload =
  | { kind: "declined" }
  | { kind: "completed"; workspace: WorkspacePayload; changed: boolean };

export type GitPushResultPayload =
  | { kind: "declined" }
  | { kind: "completed"; pushed: boolean };

export type GitClonePlanPayload = {
  planToken: string;
  transport: "https" | "ssh";
  host: string;
};

export type GitCloneApplyResultPayload =
  | { validated: false; stage: "declined" }
  | { validated: true; stage: "validated" };

export type GitCloneCancelResultPayload = {
  cancellationRequested: boolean;
};

export type GitCloneCompleteResultPayload =
  | { kind: "declined" }
  | { kind: "completed"; workspace: WorkspacePayload };

export type WorkspaceWatchEventPayload = {
  workspaceId: string;
  watchSession: string;
  kind: "changed" | "degraded";
};

export const SEARCH_MATCH_FIELDS = [
  "title",
  "alias",
  "command",
  "path",
  "error",
  "tag",
  "category",
  "summary",
  "heading",
  "body",
] as const;

export type SearchMatchField = (typeof SEARCH_MATCH_FIELDS)[number];
export type SearchScope =
  | "active"
  | "tested"
  | "draft"
  | "stale"
  | "archived"
  | "all";

export type SearchHitPayload = {
  guideId: string;
  excerpt: string;
  excerptField: SearchMatchField;
  matchedFields: SearchMatchField[];
  highlights: Array<{ start: number; end: number }>;
};

export const WORKSPACE_INVALIDATED_EVENT = "rehow://workspace-invalidated/v1";

export function isWorkspaceWatchEvent(
  value: unknown,
): value is WorkspaceWatchEventPayload {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as {
    workspaceId?: unknown;
    watchSession?: unknown;
    kind?: unknown;
  };
  return (
    Object.keys(candidate).length === 3 &&
    typeof candidate.workspaceId === "string" &&
    candidate.workspaceId.length <= 64 &&
    typeof candidate.watchSession === "string" &&
    /^[1-9][0-9]{0,19}$/.test(candidate.watchSession) &&
    (candidate.kind === "changed" || candidate.kind === "degraded")
  );
}

export function isSearchHitPayload(value: unknown): value is SearchHitPayload {
  if (typeof value !== "object" || value === null) return false;
  const hit = value as {
    guideId?: unknown;
    excerpt?: unknown;
    excerptField?: unknown;
    matchedFields?: unknown;
    highlights?: unknown;
  };
  if (
    Object.keys(hit).length !== 5 ||
    typeof hit.guideId !== "string" ||
    hit.guideId.length > 64 ||
    typeof hit.excerpt !== "string" ||
    Array.from(hit.excerpt).length > 222 ||
    !SEARCH_MATCH_FIELDS.includes(hit.excerptField as SearchMatchField) ||
    !Array.isArray(hit.matchedFields) ||
    hit.matchedFields.length > SEARCH_MATCH_FIELDS.length ||
    !hit.matchedFields.every((field) =>
      SEARCH_MATCH_FIELDS.includes(field as SearchMatchField),
    ) ||
    !Array.isArray(hit.highlights) ||
    hit.highlights.length > 32
  )
    return false;
  const excerptLength = Array.from(hit.excerpt).length;
  let previousEnd = 0;
  return hit.highlights.every((value) => {
    if (
      typeof value !== "object" ||
      value === null ||
      Object.keys(value).length !== 2
    )
      return false;
    const range = value as { start?: unknown; end?: unknown };
    if (
      !Number.isInteger(range.start) ||
      !Number.isInteger(range.end) ||
      (range.start as number) < previousEnd ||
      (range.end as number) <= (range.start as number) ||
      (range.end as number) > excerptLength
    )
      return false;
    previousEnd = range.end as number;
    return true;
  });
}

export function isMarkdownImportPreviewPayload(
  value: unknown,
): value is MarkdownImportPreviewPayload {
  if (typeof value !== "object" || value === null) return false;
  const preview = value as Partial<MarkdownImportPreviewPayload> &
    Record<string, unknown>;
  const boundedCount = (count: unknown, maximum: number) =>
    Number.isSafeInteger(count) &&
    (count as number) >= 0 &&
    (count as number) <= maximum;
  if (
    Object.keys(preview).length !== 7 ||
    !boundedCount(preview.included, 10_000) ||
    !boundedCount(preview.normalizationRequired, preview.included as number) ||
    !boundedCount(preview.excludedNonMarkdown, 20_000) ||
    !boundedCount(preview.malformedOrUnsafe, 20_000) ||
    !boundedCount(preview.oversized, 20_000) ||
    typeof preview.aggregateBytes !== "string" ||
    !/^(0|[1-9][0-9]{0,7})$/.test(preview.aggregateBytes) ||
    Number(preview.aggregateBytes) > 50 * 1024 * 1024 ||
    !Array.isArray(preview.entries) ||
    preview.entries.length > 100 ||
    preview.entries.length > (preview.included as number)
  )
    return false;
  if (
    (preview.included as number) +
      (preview.excludedNonMarkdown as number) +
      (preview.malformedOrUnsafe as number) +
      (preview.oversized as number) >
    20_000
  )
    return false;
  const displayPaths = new Set<string>();
  return preview.entries.every((value) => {
    if (typeof value !== "object" || value === null) return false;
    const entry = value as Partial<
      MarkdownImportPreviewPayload["entries"][number]
    > &
      Record<string, unknown>;
    const valid =
      Object.keys(entry).length === 3 &&
      isBoundedText(entry.displayPath, 1, 3072) &&
      /^(?:[ -$&-~]|%[0-9A-F]{2})+$/.test(entry.displayPath as string) &&
      typeof entry.size === "string" &&
      /^(0|[1-9][0-9]{0,7})$/.test(entry.size) &&
      Number(entry.size) <= 2 * 1024 * 1024 - 4 * 1024 &&
      typeof entry.normalizationRequired === "boolean";
    if (!valid || displayPaths.has(entry.displayPath as string)) return false;
    displayPaths.add(entry.displayPath as string);
    return true;
  });
}

export function highlightedExcerptSegments(hit: SearchHitPayload) {
  const characters = Array.from(hit.excerpt);
  const segments: Array<{
    start: number;
    end: number;
    text: string;
    highlighted: boolean;
  }> = [];
  let cursor = 0;
  for (const range of hit.highlights) {
    if (range.start > cursor) {
      segments.push({
        start: cursor,
        end: range.start,
        text: characters.slice(cursor, range.start).join(""),
        highlighted: false,
      });
    }
    segments.push({
      start: range.start,
      end: range.end,
      text: characters.slice(range.start, range.end).join(""),
      highlighted: true,
    });
    cursor = range.end;
  }
  if (cursor < characters.length) {
    segments.push({
      start: cursor,
      end: characters.length,
      text: characters.slice(cursor).join(""),
      highlighted: false,
    });
  }
  return segments;
}

export type OpenWorkspaceResult =
  | { kind: "cancelled" }
  | { kind: "empty"; facts: FolderFacts }
  | { kind: "compatible"; workspace: WorkspacePayload }
  | { kind: "newerUnsupported"; version: number; facts: FolderFacts }
  | {
      kind: "ordinaryMarkdown";
      facts: FolderFacts;
      preview: MarkdownImportPreviewPayload;
    }
  | { kind: "unrelatedNonempty"; facts: FolderFacts }
  | { kind: "malformed"; reason: string; facts: FolderFacts };

export type MarkdownImportPreviewPayload = {
  included: number;
  normalizationRequired: number;
  excludedNonMarkdown: number;
  malformedOrUnsafe: number;
  oversized: number;
  aggregateBytes: string;
  entries: Array<{
    displayPath: string;
    size: string;
    normalizationRequired: boolean;
  }>;
};

export type MarkdownImportDestinationResult =
  | { kind: "cancelled" }
  | { kind: "selected"; facts: FolderFacts };

export type MarkdownImportApplyPayload = {
  workspace: WorkspacePayload;
  imported: number;
  total: number;
  complete: boolean;
  failure: string | null;
};

export type SaveGuidePayload = {
  workspace: WorkspacePayload;
  guide: GuideBodyPayload;
  changed: boolean;
};

export type RemoveGuidePayload = {
  workspace: WorkspacePayload;
  removed: boolean;
};

export type CreateCategoryPayload = {
  workspace: WorkspacePayload;
  categoryId: string;
};

export type CreateGuidePayload = {
  workspace: WorkspacePayload;
  guide: GuideBodyPayload;
};

export type SaveCategoryPayload = {
  workspace: WorkspacePayload;
  changed: boolean;
};

export function isGitStatusPayload(value: unknown): value is GitStatusPayload {
  if (typeof value !== "object" || value === null) return false;
  const status = value as Partial<Record<keyof GitStatusPayload, unknown>> &
    Record<string, unknown>;
  if (
    Object.keys(status).length !== 16 ||
    ![
      "unavailable",
      "notRepository",
      "unsafe",
      "ready",
      "failed",
      "recoveryRequired",
    ].includes(String(status.kind))
  )
    return false;
  for (const key of ["version", "branch", "upstream", "failure"] as const) {
    const field = status[key];
    if (field !== null && (typeof field !== "string" || field.length > 512))
      return false;
  }
  if (
    typeof status.detached !== "boolean" ||
    typeof status.divergenceKnown !== "boolean" ||
    ![null, "configured", "missing", "invalid"].includes(
      status.identity as null | string,
    ) ||
    !["available", "notConfigured", "unsafe"].includes(
      String(status.globalIdentity),
    )
  )
    return false;
  for (const key of [
    "ahead",
    "behind",
    "changed",
    "untracked",
    "conflicts",
    "remoteCount",
  ] as const) {
    const count = status[key];
    if (
      typeof count !== "number" ||
      !Number.isInteger(count) ||
      count < 0 ||
      count > 0xffff_ffff
    )
      return false;
  }
  return (
    Array.isArray(status.remotes) &&
    status.remotes.length <= 32 &&
    status.remotes.every(
      (remote) => typeof remote === "string" && remote.length <= 512,
    )
  );
}

export function isGitInitPlanPayload(
  value: unknown,
): value is GitInitPlanPayload {
  if (typeof value !== "object" || value === null) return false;
  const plan = value as Partial<GitInitPlanPayload> & Record<string, unknown>;
  return (
    Object.keys(plan).length === 2 &&
    typeof plan.planToken === "string" &&
    /^plan:v1:[A-Za-z0-9_-]{43}$/.test(plan.planToken) &&
    plan.initialBranch === "main"
  );
}

export function isGitInitResultPayload(
  value: unknown,
): value is GitInitResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitInitResultPayload> &
    Record<string, unknown>;
  return (
    Object.keys(result).length === 1 && typeof result.initialized === "boolean"
  );
}

export function isGitRemotePlanPayload(
  value: unknown,
): value is GitRemotePlanPayload {
  if (typeof value !== "object" || value === null) return false;
  const plan = value as Partial<GitRemotePlanPayload> & Record<string, unknown>;
  if (Object.keys(plan).length !== 4) return false;
  return (
    typeof plan.planToken === "string" &&
    /^plan:v1:[A-Za-z0-9_-]{43}$/.test(plan.planToken) &&
    typeof plan.remoteName === "string" &&
    plan.remoteName.length >= 1 &&
    plan.remoteName.length <= 64 &&
    (plan.transport === "https" || plan.transport === "ssh") &&
    typeof plan.host === "string" &&
    plan.host.length >= 1 &&
    plan.host.length <= 253
  );
}

export function isGitRemoteResultPayload(
  value: unknown,
): value is GitRemoteResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitRemoteResultPayload> &
    Record<string, unknown>;
  return (
    Object.keys(result).length === 1 && typeof result.connected === "boolean"
  );
}

export function isGitIdentityPlanPayload(
  value: unknown,
): value is GitIdentityPlanPayload {
  if (typeof value !== "object" || value === null) return false;
  const plan = value as Partial<GitIdentityPlanPayload> &
    Record<string, unknown>;
  return (
    Object.keys(plan).length === 2 &&
    typeof plan.planToken === "string" &&
    /^plan:v1:[A-Za-z0-9_-]{43}$/.test(plan.planToken) &&
    (plan.source === "manual" || plan.source === "global")
  );
}

export function isGitIdentityResultPayload(
  value: unknown,
): value is GitIdentityResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitIdentityResultPayload> &
    Record<string, unknown>;
  return (
    Object.keys(result).length === 1 && typeof result.configured === "boolean"
  );
}

export function isUint32(value: unknown) {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 0 &&
    value <= 0xffff_ffff
  );
}

export function isUnsafeDisplayCodePoint(code: number) {
  return (
    code <= 0x1f ||
    (code >= 0x7f && code <= 0x9f) ||
    code === 0x2028 ||
    code === 0x2029 ||
    (code >= 0x202a && code <= 0x202e) ||
    (code >= 0x2066 && code <= 0x2069) ||
    (code >= 0xfdd0 && code <= 0xfdef) ||
    (code & 0xffff) === 0xfffe ||
    (code & 0xffff) === 0xffff
  );
}

export function containsUnsafeDisplayCodePoint(value: string) {
  return Array.from(value).some((scalar) =>
    isUnsafeDisplayCodePoint(scalar.codePointAt(0) ?? 0),
  );
}

export function isGitCommitPlanPayload(
  value: unknown,
): value is GitCommitPlanPayload {
  if (typeof value !== "object" || value === null) return false;
  const plan = value as Partial<GitCommitPlanPayload> & Record<string, unknown>;
  return (
    Object.keys(plan).length === 6 &&
    typeof plan.planToken === "string" &&
    /^plan:v1:[A-Za-z0-9_-]{43}$/.test(plan.planToken) &&
    typeof plan.branch === "string" &&
    plan.branch.length >= 1 &&
    plan.branch.length <= 4096 &&
    !containsUnsafeDisplayCodePoint(plan.branch) &&
    isUint32(plan.created) &&
    isUint32(plan.updated) &&
    isUint32(plan.deleted) &&
    isUint32(plan.unrelatedChanges)
  );
}

export function isGitCommitResultPayload(
  value: unknown,
): value is GitCommitResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitCommitResultPayload> &
    Record<string, unknown>;
  return (
    Object.keys(result).length === 2 &&
    typeof result.committed === "boolean" &&
    typeof result.workspaceChanged === "boolean" &&
    (!result.workspaceChanged || result.committed)
  );
}

export function isGitCommitCancelResultPayload(
  value: unknown,
): value is GitCommitCancelResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitCommitCancelResultPayload> &
    Record<string, unknown>;
  return Object.keys(result).length === 1 && result.cancelled === true;
}

export function isGitCommitRecoveryResultPayload(
  value: unknown,
): value is GitCommitRecoveryResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitCommitRecoveryResultPayload> &
    Record<string, unknown>;
  return (
    Object.keys(result).length === 2 &&
    typeof result.recovered === "boolean" &&
    typeof result.workspaceChanged === "boolean" &&
    (!result.workspaceChanged || result.recovered)
  );
}

export function isGitSyncInspectionPayload(
  value: unknown,
): value is GitSyncInspectionPayload {
  if (typeof value !== "object" || value === null) return false;
  const inspection = value as Partial<GitSyncInspectionPayload> &
    Record<string, unknown>;
  return (
    Object.keys(inspection).length === 7 &&
    typeof inspection.planToken === "string" &&
    /^plan:v1:[A-Za-z0-9_-]{43}$/.test(inspection.planToken) &&
    isBoundedText(inspection.remoteName, 1, 64) &&
    !containsUnsafeDisplayCodePoint(String(inspection.remoteName)) &&
    inspection.transport === "https" &&
    isBoundedText(inspection.host, 1, 253) &&
    !containsUnsafeDisplayCodePoint(String(inspection.host)) &&
    [
      "upToDate",
      "localAhead",
      "remoteFastForward",
      "diverged",
      "historyLimit",
    ].includes(String(inspection.ancestry)) &&
    typeof inspection.localChanges === "boolean" &&
    typeof inspection.workspaceMatchesRemote === "boolean"
  );
}

export function isGitSyncCancelResultPayload(
  value: unknown,
): value is GitSyncCancelResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitSyncCancelResultPayload> &
    Record<string, unknown>;
  return (
    Object.keys(result).length === 1 &&
    typeof result.cancellationRequested === "boolean"
  );
}

export function isGitSyncApplyResultPayload(
  value: unknown,
): value is GitSyncApplyResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as {
    kind?: unknown;
    workspace?: unknown;
    changed?: unknown;
  };
  if (result.kind === "declined") return Object.keys(result).length === 1;
  return (
    result.kind === "completed" &&
    Object.keys(result).length === 3 &&
    isWorkspacePayload(result.workspace) &&
    typeof result.changed === "boolean"
  );
}

export function isGitPushResultPayload(
  value: unknown,
): value is GitPushResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as { kind?: unknown; pushed?: unknown };
  if (result.kind === "declined") return Object.keys(result).length === 1;
  return (
    result.kind === "completed" &&
    Object.keys(result).length === 2 &&
    result.pushed === true
  );
}

export function isGitClonePlanPayload(
  value: unknown,
): value is GitClonePlanPayload {
  if (typeof value !== "object" || value === null) return false;
  const plan = value as Partial<GitClonePlanPayload> & Record<string, unknown>;
  return (
    Object.keys(plan).length === 3 &&
    typeof plan.planToken === "string" &&
    /^plan:v1:[A-Za-z0-9_-]{43}$/.test(plan.planToken) &&
    (plan.transport === "https" || plan.transport === "ssh") &&
    typeof plan.host === "string" &&
    plan.host.length >= 1 &&
    plan.host.length <= 253
  );
}

export function isGitCloneApplyResultPayload(
  value: unknown,
): value is GitCloneApplyResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitCloneApplyResultPayload> &
    Record<string, unknown>;
  if (Object.keys(result).length !== 2) return false;
  return (
    (result.validated === false && result.stage === "declined") ||
    (result.validated === true && result.stage === "validated")
  );
}

export function isGitCloneCancelResultPayload(
  value: unknown,
): value is GitCloneCancelResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<GitCloneCancelResultPayload> &
    Record<string, unknown>;
  return (
    Object.keys(result).length === 1 &&
    typeof result.cancellationRequested === "boolean"
  );
}

export function isBoundedText(
  value: unknown,
  minimum: number,
  maximum: number,
) {
  if (
    typeof value !== "string" ||
    value.length < minimum ||
    value.length > maximum * 2
  )
    return false;
  const length = Array.from(value).length;
  return length >= minimum && length <= maximum;
}

export function isSafeAttachmentPreviewText(value: string) {
  for (const character of value) {
    const code = character.codePointAt(0);
    if (code === undefined) return false;
    if (
      (code <= 0x1f && code !== 0x09 && code !== 0x0a) ||
      (code >= 0x7f && code <= 0x9f) ||
      (code >= 0xd800 && code <= 0xdfff) ||
      isUnicode16InvisibleOrFormat(code) ||
      (code >= 0x2028 && code <= 0x2029) ||
      (code >= 0xfdd0 && code <= 0xfdef) ||
      (code & 0xffff) === 0xfffe ||
      (code & 0xffff) === 0xffff
    )
      return false;
  }
  return true;
}

export function isUnicode16InvisibleOrFormat(code: number) {
  return (
    code === 0x00ad ||
    code === 0x034f ||
    (code >= 0x0600 && code <= 0x0605) ||
    code === 0x061c ||
    code === 0x06dd ||
    code === 0x070f ||
    (code >= 0x0890 && code <= 0x0891) ||
    code === 0x08e2 ||
    (code >= 0x115f && code <= 0x1160) ||
    (code >= 0x17b4 && code <= 0x17b5) ||
    (code >= 0x180b && code <= 0x180f) ||
    (code >= 0x200b && code <= 0x200f) ||
    (code >= 0x202a && code <= 0x202e) ||
    (code >= 0x2060 && code <= 0x206f) ||
    code === 0x3164 ||
    (code >= 0xfe00 && code <= 0xfe0f) ||
    code === 0xfeff ||
    code === 0xffa0 ||
    (code >= 0xfff0 && code <= 0xfffb) ||
    code === 0x110bd ||
    code === 0x110cd ||
    (code >= 0x13430 && code <= 0x1343f) ||
    (code >= 0x1bca0 && code <= 0x1bca3) ||
    (code >= 0x1d173 && code <= 0x1d17a) ||
    (code >= 0xe0000 && code <= 0xe0fff)
  );
}

export function isAttachmentTextPreviewPayload(
  value: unknown,
): value is AttachmentTextPreviewPayload {
  if (typeof value !== "object" || value === null) return false;
  const preview = value as Partial<AttachmentTextPreviewPayload> &
    Record<string, unknown>;
  return (
    Object.keys(preview).length === 4 &&
    isBoundedText(preview.title, 1, 200) &&
    isSafeAttachmentPreviewText(preview.title as string) &&
    isBoundedText(preview.text, 0, 1024 * 1024) &&
    isSafeAttachmentPreviewText(preview.text as string) &&
    Number.isSafeInteger(preview.sizeBytes) &&
    (preview.sizeBytes as number) >= 0 &&
    (preview.sizeBytes as number) <= 1024 * 1024 &&
    typeof preview.sanitized === "boolean"
  );
}

export function isAttachmentExportPayload(
  value: unknown,
): value is AttachmentExportPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<AttachmentExportPayload> &
    Record<string, unknown>;
  if (result.kind === "cancelled") return Object.keys(result).length === 1;
  return (
    result.kind === "exported" &&
    Object.keys(result).length === 2 &&
    Number.isSafeInteger(result.sizeBytes) &&
    (result.sizeBytes as number) >= 0 &&
    (result.sizeBytes as number) <= 64 * 1024 * 1024
  );
}

export function isAttachmentRevealPayload(
  value: unknown,
): value is AttachmentRevealPayload {
  const result = value as Partial<AttachmentRevealPayload> &
    Record<string, unknown>;
  return (
    typeof value === "object" &&
    value !== null &&
    Object.keys(value).length === 1 &&
    result.revealed === true
  );
}

export function isExternalLinkOpenPayload(
  value: unknown,
): value is ExternalLinkOpenPayload {
  const result = value as Partial<ExternalLinkOpenPayload> &
    Record<string, unknown>;
  return (
    typeof value === "object" &&
    value !== null &&
    Object.keys(value).length === 1 &&
    result.requested === true
  );
}

export function isWorkspacePayload(value: unknown): value is WorkspacePayload {
  if (typeof value !== "object" || value === null) return false;
  const workspace = value as Partial<WorkspacePayload> &
    Record<string, unknown>;
  if (
    Object.keys(workspace).length !== 10 ||
    !isBoundedText(workspace.id, 1, 64) ||
    !isBoundedText(workspace.name, 1, 200) ||
    !Array.isArray(workspace.categories) ||
    workspace.categories.length > 10_000 ||
    !Array.isArray(workspace.guides) ||
    workspace.guides.length > 10_000 ||
    !Number.isSafeInteger(workspace.issueCount) ||
    (workspace.issueCount as number) < 0 ||
    (workspace.issueCount as number) > 10_000 ||
    !Array.isArray(workspace.issues) ||
    workspace.issues.length > 100 ||
    typeof workspace.issuesTruncated !== "boolean" ||
    (workspace.watchStatus !== "active" &&
      workspace.watchStatus !== "degraded") ||
    typeof workspace.watchSession !== "string" ||
    !/^[1-9][0-9]{0,19}$/.test(workspace.watchSession)
  )
    return false;
  const issuesValid = workspace.issues.every((value) => {
    if (typeof value !== "object" || value === null) return false;
    const issue = value as WorkspaceIssuePayload & Record<string, unknown>;
    return (
      Object.keys(issue).length === 6 &&
      WORKSPACE_ISSUE_KINDS.includes(issue.kind) &&
      (issue.diagnostic === null ||
        (typeof issue.diagnostic === "string" &&
          /^[a-z][A-Za-z]{0,63}$/.test(issue.diagnostic))) &&
      (issue.field === null ||
        (typeof issue.field === "string" &&
          /^[a-z_]{1,64}$/.test(issue.field))) &&
      (issue.line === null || isUint32(issue.line)) &&
      (issue.column === null || isUint32(issue.column)) &&
      isBoundedText(issue.relativePath, 1, 2_048) &&
      !containsUnsafeDisplayCodePoint(issue.relativePath)
    );
  });
  if (
    !issuesValid ||
    (workspace.issueCount as number) < workspace.issues.length ||
    workspace.issuesTruncated !==
      (workspace.issueCount as number) > workspace.issues.length
  )
    return false;
  if (typeof workspace.facts !== "object" || workspace.facts === null)
    return false;
  const facts = workspace.facts as FolderFacts & Record<string, unknown>;
  if (
    Object.keys(facts).length !== 5 ||
    ![
      facts.writable,
      facts.existingGitRepository,
      facts.nestedGitRepository,
      facts.gitProbeComplete,
      facts.unsafeGitMarker,
    ].every((fact) => typeof fact === "boolean")
  )
    return false;
  const categoriesValid = workspace.categories.every((value) => {
    if (typeof value !== "object" || value === null) return false;
    const category = value as WorkspacePayload["categories"][number] &
      Record<string, unknown>;
    return (
      Object.keys(category).length === 6 &&
      isBoundedText(category.id, 1, 64) &&
      isBoundedText(category.name, 1, 200) &&
      (category.summary === null || isBoundedText(category.summary, 1, 500)) &&
      (category.status === "active" || category.status === "archived") &&
      isBoundedText(category.revision, 1, 128) &&
      Number.isSafeInteger(category.guideCount) &&
      category.guideCount >= 0 &&
      category.guideCount <= 10_000
    );
  });
  const guidesValid = workspace.guides.every((value) => {
    if (typeof value !== "object" || value === null) return false;
    const guide = value as GuideSummary & Record<string, unknown>;
    return (
      Object.keys(guide).length === 12 &&
      isBoundedText(guide.id, 1, 64) &&
      (guide.origin === "managed" || guide.origin === "unadopted") &&
      isBoundedText(guide.title, 1, 200) &&
      isBoundedText(guide.summary, 0, 500) &&
      isBoundedText(guide.categoryId, 1, 64) &&
      isBoundedText(guide.category, 1, 200) &&
      ["Tested", "Draft", "Stale", "Archived"].includes(guide.status) &&
      isBoundedText(guide.updatedAt, 1, 64) &&
      Array.isArray(guide.tags) &&
      guide.tags.length <= 64 &&
      guide.tags.every((tag) => isBoundedText(tag, 1, 64)) &&
      Array.isArray(guide.aliases) &&
      guide.aliases.length <= 64 &&
      guide.aliases.every((alias) => isBoundedText(alias, 1, 200)) &&
      Array.isArray(guide.attachments) &&
      guide.attachments.length <= 10_000 &&
      guide.attachments.every((value) => {
        if (typeof value !== "object" || value === null) return false;
        const attachment = value as GuideSummary["attachments"][number] &
          Record<string, unknown>;
        return (
          Object.keys(attachment).length === 3 &&
          isBoundedText(attachment.id, 1, 64) &&
          isBoundedText(attachment.title, 1, 200) &&
          (attachment.sizeBytes === null ||
            (Number.isSafeInteger(attachment.sizeBytes) &&
              attachment.sizeBytes >= 0))
        );
      }) &&
      isBoundedText(guide.matches, 0, 20_000)
    );
  });
  return categoriesValid && guidesValid;
}

export function isDesktopPreferencesPayload(
  value: unknown,
): value is DesktopPreferencesPayload {
  if (typeof value !== "object" || value === null) return false;
  const preferences = value as Partial<DesktopPreferencesPayload> &
    Record<string, unknown>;
  return (
    Object.keys(preferences).length === 2 &&
    typeof preferences.syncOnStart === "boolean" &&
    (preferences.syncIntervalMinutes === null ||
      (typeof preferences.syncIntervalMinutes === "number" &&
        SYNC_INTERVALS.includes(
          preferences.syncIntervalMinutes as (typeof SYNC_INTERVALS)[number],
        )))
  );
}

export function isRecentWorkspaceResult(
  value: unknown,
): value is RecentWorkspaceResult {
  if (typeof value !== "object" || value === null) return false;
  const result = value as {
    kind?: unknown;
    workspace?: unknown;
    reason?: unknown;
  };
  if (result.kind === "none") return Object.keys(result).length === 1;
  if (result.kind === "restored") {
    return (
      Object.keys(result).length === 2 && isWorkspacePayload(result.workspace)
    );
  }
  return (
    result.kind === "unavailable" &&
    Object.keys(result).length === 2 &&
    isBoundedText(result.reason, 1, 500) &&
    !containsUnsafeDisplayCodePoint(result.reason as string)
  );
}

export function isMarkdownImportApplyPayload(
  value: unknown,
): value is MarkdownImportApplyPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as Partial<MarkdownImportApplyPayload> &
    Record<string, unknown>;
  return (
    Object.keys(result).length === 5 &&
    isWorkspacePayload(result.workspace) &&
    Number.isSafeInteger(result.imported) &&
    (result.imported as number) >= 0 &&
    Number.isSafeInteger(result.total) &&
    (result.total as number) >= 1 &&
    (result.total as number) <= 10_000 &&
    (result.imported as number) <= (result.total as number) &&
    typeof result.complete === "boolean" &&
    result.complete === ((result.imported as number) === result.total) &&
    (result.failure === null || isBoundedText(result.failure, 1, 500)) &&
    (result.complete ? result.failure === null : result.failure !== null)
  );
}

export function isGitCloneCompleteResultPayload(
  value: unknown,
): value is GitCloneCompleteResultPayload {
  if (typeof value !== "object" || value === null) return false;
  const result = value as {
    kind?: unknown;
    workspace?: unknown;
  };
  if (result.kind === "declined") return Object.keys(result).length === 1;
  return (
    result.kind === "completed" &&
    Object.keys(result).length === 2 &&
    isWorkspacePayload(result.workspace)
  );
}

export function normalizedRemoteName(value: string) {
  const normalized = value.trim();
  return /^[a-z0-9][a-z0-9._-]{0,63}$/.test(normalized) &&
    !normalized.includes("..") &&
    !normalized.endsWith(".lock")
    ? normalized
    : null;
}

export function normalizedRemoteUrl(value: string) {
  const normalized = value.trim();
  return normalized.length >= 1 && normalized.length <= 2048
    ? normalized
    : null;
}

export function normalizedGitCommitMessage(value: string) {
  const normalized = value.trim();
  const scalars = Array.from(normalized);
  if (
    scalars.length < 1 ||
    scalars.length > 200 ||
    containsUnsafeDisplayCodePoint(normalized)
  )
    return null;
  return normalized;
}

export function gitSyncPresentation(inspection: GitSyncInspectionPayload) {
  if (inspection.ancestry === "upToDate") {
    return {
      title: "Already Up to Date",
      detail: "The local branch already matches the validated upstream commit.",
      canApply: false,
      canPush: false,
    };
  }
  if (inspection.ancestry === "localAhead") {
    return {
      title: "Local Branch Is Ahead",
      detail:
        "Push the reviewed local commit with an exact, normal fast-forward update. Local uncommitted changes are not included.",
      canApply: false,
      canPush: true,
    };
  }
  if (inspection.ancestry === "diverged") {
    return {
      title: "Branches Have Diverged",
      detail:
        "Rehow will not merge automatically. Resolve the histories with Git, then inspect again.",
      canApply: false,
      canPush: false,
    };
  }
  if (inspection.ancestry === "historyLimit") {
    return {
      title: "History Needs Review",
      detail:
        "Rehow could not prove the relationship within its safety limit. Review the repository with Git.",
      canApply: false,
      canPush: false,
    };
  }
  if (inspection.localChanges) {
    return {
      title: "Commit Local Changes First",
      detail:
        "A fast-forward is available, but Rehow will not overwrite local changes. Commit them, then inspect again.",
      canApply: false,
      canPush: false,
    };
  }
  return {
    title: "Fast-Forward Available",
    detail: inspection.workspaceMatchesRemote
      ? "Workspace content already matches. Apply the validated Git metadata update."
      : "Apply the validated upstream Workspace and Git metadata update.",
    canApply: true,
    canPush: false,
  };
}

export function normalizedCloneUrl(value: string) {
  const normalized = value.trim();
  if (
    normalized.length < 1 ||
    normalized.length > 2048 ||
    !/^[\x21-\x7e]+$/.test(normalized) ||
    normalized.includes("\\") ||
    normalized.includes("%") ||
    normalized.includes("#") ||
    normalized.includes("?")
  )
    return null;
  return normalized;
}
