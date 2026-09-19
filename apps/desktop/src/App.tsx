import type { SafeMarkdownDocument } from "@rehow/contracts";
import type { FormEvent } from "react";
import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";
import { MarkdownDocument } from "./MarkdownDocument";

export const APPLICATION_NAME = "Rehow";

type GuideSummary = {
  id: string;
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

type GuideBodyPayload = {
  id: string;
  revision: string;
  body: string;
  document: SafeMarkdownDocument;
};

type FolderFacts = {
  writable: boolean;
  existingGitRepository: boolean;
  nestedGitRepository: boolean;
  gitProbeComplete: boolean;
  unsafeGitMarker: boolean;
};

type WorkspacePayload = {
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
  facts: FolderFacts;
  watchStatus: "active" | "degraded";
  watchSession: string;
};

type GitStatusPayload = {
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
  failure: string | null;
};

type GitInitPlanPayload = {
  planToken: string;
  initialBranch: "main";
};

type GitInitResultPayload = {
  initialized: boolean;
};

type GitRemotePlanPayload = {
  planToken: string;
  remoteName: string;
  transport: "https" | "ssh";
  host: string;
};

type GitRemoteResultPayload = {
  connected: boolean;
};

type GitCommitPlanPayload = {
  planToken: string;
  branch: string;
  created: number;
  updated: number;
  deleted: number;
  unrelatedChanges: number;
};

type GitCommitResultPayload = {
  committed: boolean;
  workspaceChanged: boolean;
};

type GitCommitCancelResultPayload = {
  cancelled: boolean;
};

type GitCommitRecoveryResultPayload = {
  recovered: boolean;
  workspaceChanged: boolean;
};

type GitCommitStage =
  | "idle"
  | "planning"
  | "applying"
  | "cancelling"
  | "recovering";

type GitSyncAncestry =
  | "upToDate"
  | "localAhead"
  | "remoteFastForward"
  | "diverged"
  | "historyLimit";

type GitSyncInspectionPayload = {
  planToken: string;
  remoteName: string;
  transport: "https";
  host: string;
  ancestry: GitSyncAncestry;
  localChanges: boolean;
  workspaceMatchesRemote: boolean;
};

type GitSyncCancelResultPayload = {
  cancellationRequested: boolean;
};

type GitSyncApplyResultPayload =
  | { kind: "declined" }
  | { kind: "completed"; workspace: WorkspacePayload; changed: boolean };

type GitPushResultPayload =
  | { kind: "declined" }
  | { kind: "completed"; pushed: boolean };

type NativeInvoker = (
  command: string,
  arguments_?: Record<string, unknown>,
) => Promise<unknown>;

type GitSyncStage =
  | "idle"
  | "inspecting"
  | "cancelling"
  | "applying"
  | "recovering";

type GitClonePlanPayload = {
  planToken: string;
  transport: "https" | "ssh";
  host: string;
};

type GitCloneApplyResultPayload =
  | { validated: false; stage: "declined" }
  | { validated: true; stage: "validated" };

type GitCloneCancelResultPayload = {
  cancellationRequested: boolean;
};

type GitCloneCompleteResultPayload =
  | { kind: "declined" }
  | { kind: "completed"; workspace: WorkspacePayload };

type GitCloneStage =
  | "idle"
  | "entry"
  | "validating"
  | "cancelling"
  | "validated"
  | "recovery"
  | "completing";

type WorkspaceWatchEventPayload = {
  workspaceId: string;
  watchSession: string;
  kind: "changed" | "degraded";
};

const SEARCH_MATCH_FIELDS = [
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

type SearchMatchField = (typeof SEARCH_MATCH_FIELDS)[number];
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

const WORKSPACE_INVALIDATED_EVENT = "rehow://workspace-invalidated/v1";

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

export function automaticRefreshBlocked(states: {
  busy: boolean;
  dirtyGuide: boolean;
  dirtyGuideDetails: boolean;
  dirtyCategoryRename: boolean;
  dirtyCategoryCreate: boolean;
  dirtyGuideCreate: boolean;
  dirtyGitCommit: boolean;
}) {
  return Object.values(states).some(Boolean);
}

export function isCurrentWorkspaceWatchEvent(
  event: WorkspaceWatchEventPayload,
  workspaceId: string,
  watchSession: string,
) {
  return (
    workspaceId !== "" &&
    event.workspaceId === workspaceId &&
    event.watchSession === watchSession
  );
}

export function refreshResultMayApply(
  startedEditGeneration: number,
  currentEditGeneration: number,
) {
  return startedEditGeneration === currentEditGeneration;
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

type OpenWorkspaceResult =
  | { kind: "cancelled" }
  | { kind: "empty"; facts: FolderFacts }
  | { kind: "compatible"; workspace: WorkspacePayload }
  | { kind: "newerUnsupported"; version: number; facts: FolderFacts }
  | { kind: "ordinaryMarkdown"; facts: FolderFacts }
  | { kind: "unrelatedNonempty"; facts: FolderFacts }
  | { kind: "malformed"; reason: string; facts: FolderFacts };

type SaveGuidePayload = {
  workspace: WorkspacePayload;
  guide: GuideBodyPayload;
  changed: boolean;
};

type RemoveGuidePayload = {
  workspace: WorkspacePayload;
  removed: boolean;
};

type CreateCategoryPayload = {
  workspace: WorkspacePayload;
  categoryId: string;
};

type CreateGuidePayload = {
  workspace: WorkspacePayload;
  guide: GuideBodyPayload;
};

type SaveCategoryPayload = {
  workspace: WorkspacePayload;
  changed: boolean;
};

type IconName =
  | "book"
  | "search"
  | "plus"
  | "chevron"
  | "folder"
  | "tag"
  | "archive"
  | "refresh"
  | "settings";

function Icon({ name }: { name: IconName }) {
  const paths: Record<IconName, React.ReactNode> = {
    book: (
      <>
        <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H11v14H6.5A2.5 2.5 0 0 0 4 19.5z" />
        <path d="M20 5.5A2.5 2.5 0 0 0 17.5 3H13v14h4.5a2.5 2.5 0 0 1 2.5 2.5z" />
      </>
    ),
    search: (
      <>
        <circle cx="11" cy="11" r="6.5" />
        <path d="m16 16 4 4" />
      </>
    ),
    plus: (
      <>
        <path d="M12 5v14M5 12h14" />
      </>
    ),
    chevron: <path d="m9 18 6-6-6-6" />,
    folder: (
      <path d="M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2h8.5A1.5 1.5 0 0 1 21 9.5v8a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" />
    ),
    tag: (
      <>
        <path d="M20 13 13 20l-9-9V4h7z" />
        <circle cx="8.5" cy="8.5" r="1" />
      </>
    ),
    archive: (
      <>
        <path d="M4 7h16v13H4zM3 4h18v4H3z" />
        <path d="M9 12h6" />
      </>
    ),
    refresh: (
      <>
        <path d="M20 7v5h-5" />
        <path d="M4 17v-5h5" />
        <path d="M6.1 9a7 7 0 0 1 11.6-2L20 9" />
        <path d="m4 15 2.3 2a7 7 0 0 0 11.6-2" />
      </>
    ),
    settings: (
      <>
        <circle cx="12" cy="12" r="3" />
        <path d="M19 15.2a2 2 0 0 0 .4 2.2l-2 2a2 2 0 0 0-2.2-.4 2 2 0 0 0-1.2 1.8h-4A2 2 0 0 0 8.8 19a2 2 0 0 0-2.2.4l-2-2a2 2 0 0 0 .4-2.2A2 2 0 0 0 3.2 14v-4A2 2 0 0 0 5 8.8a2 2 0 0 0-.4-2.2l2-2a2 2 0 0 0 2.2.4A2 2 0 0 0 10 3.2h4A2 2 0 0 0 15.2 5a2 2 0 0 0 2.2-.4l2 2a2 2 0 0 0-.4 2.2 2 2 0 0 0 1.8 1.2v4a2 2 0 0 0-1.8 1.2Z" />
      </>
    ),
  };
  return (
    <svg
      aria-hidden="true"
      className="icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {paths[name]}
    </svg>
  );
}

function formatUpdated(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(new Date(value));
}

function formatLongDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "long",
    day: "numeric",
  }).format(new Date(value));
}

function formatBytes(value: number | null) {
  if (value === null) return "Size unavailable";
  const format = new Intl.NumberFormat(undefined, {
    maximumFractionDigits: value < 1_024 ? 0 : 1,
  });
  if (value < 1_024) return `${format.format(value)}\u00a0bytes`;
  if (value < 1_048_576) return `${format.format(value / 1_024)}\u00a0KiB`;
  return `${format.format(value / 1_048_576)}\u00a0MiB`;
}

export function filterGuides(guides: GuideSummary[], query: string) {
  const terms = query.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return guides;
  return guides.filter((guide) => {
    const text =
      `${guide.title} ${guide.summary} ${guide.category} ${guide.matches}`.toLocaleLowerCase();
    return terms.every((term) => text.includes(term));
  });
}

export function guideLifecycleLabel(
  status: GuideSummary["status"],
  busy: boolean,
) {
  if (status === "Archived") return busy ? "Restoring…" : "Restore Guide";
  return busy ? "Archiving…" : "Archive Guide";
}

export function canMarkGuideTested(status: GuideSummary["status"]) {
  return status === "Draft" || status === "Stale";
}

export function refreshedGuideId(
  currentId: string,
  guides: ReadonlyArray<{ id: string }>,
) {
  return guides.some(({ id }) => id === currentId)
    ? currentId
    : (guides[0]?.id ?? "");
}

export function rankedGuides(
  guides: GuideSummary[],
  query: string,
  rankedHits: SearchHitPayload[] | null,
) {
  if (query.trim() === "") return guides;
  if (rankedHits === null) return filterGuides(guides, query);
  const byId = new Map(guides.map((guide) => [guide.id, guide]));
  return rankedHits.flatMap((hit) => {
    const guide = byId.get(hit.guideId);
    return guide ? [guide] : [];
  });
}

export function guideMatchesSearchScope(
  status: GuideSummary["status"],
  scope: SearchScope,
) {
  if (scope === "all") return true;
  if (scope === "active") return status !== "Archived";
  return status.toLowerCase() === scope;
}

function isSearchScope(value: string | null): value is SearchScope {
  return ["active", "tested", "draft", "stale", "archived", "all"].includes(
    value ?? "",
  );
}

export function searchStatePath(
  pathname: string,
  search: string,
  hash: string,
  query: string,
  scope: SearchScope,
) {
  const parameters = new URLSearchParams(search);
  if (query === "") parameters.delete("q");
  else parameters.set("q", query);
  if (scope === "active") parameters.delete("scope");
  else parameters.set("scope", scope);
  const nextSearch = parameters.toString();
  return `${pathname}${nextSearch === "" ? "" : `?${nextSearch}`}${hash}`;
}

export function resultCountLabel(count: number) {
  return `${count} ${count === 1 ? "Guide" : "Guides"}`;
}

export function hasUnsavedChanges(original: string, draft: string) {
  return original !== draft;
}

export function movedCategoryOrder<T extends { id: string }>(
  categories: readonly T[],
  categoryId: string,
  direction: "up" | "down",
) {
  const index = categories.findIndex((category) => category.id === categoryId);
  const target = direction === "up" ? index - 1 : index + 1;
  if (index < 0 || target < 0 || target >= categories.length) return null;
  const moved = [...categories];
  const current = moved[index];
  const neighbor = moved[target];
  if (current === undefined || neighbor === undefined) return null;
  moved[index] = neighbor;
  moved[target] = current;
  return moved;
}

export function normalizedWorkspaceName(value: string) {
  const name = value.trim();
  const length = Array.from(name).length;
  return length >= 1 && length <= 200 ? name : null;
}

export function nativeCommandError(error: unknown) {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof error.code === "string" &&
    typeof error.message === "string"
  ) {
    return { code: error.code, message: error.message };
  }
  return {
    code: "unknown",
    message:
      "The operation could not be completed. Reopen the Workspace and try again.",
  };
}

export function normalizedCategoryDetails(
  nameValue: string,
  summaryValue: string,
) {
  const name = nameValue.trim();
  const summary = summaryValue.trim();
  if (Array.from(name).length < 1 || Array.from(name).length > 200) return null;
  if (Array.from(summary).length > 500) return null;
  return { name, summary: summary === "" ? null : summary };
}

export function normalizedGuideDetails(
  categoryId: string,
  titleValue: string,
  summaryValue: string,
) {
  const title = titleValue.trim();
  const summary = summaryValue.trim();
  if (categoryId === "") return null;
  if (Array.from(title).length < 1 || Array.from(title).length > 200)
    return null;
  if (Array.from(summary).length < 1 || Array.from(summary).length > 500)
    return null;
  return { categoryId, title, summary };
}

export function normalizedGuideMetadata(
  titleValue: string,
  summaryValue: string,
  tagsValue: string,
  aliasesValue: string,
  categoryId: string,
) {
  const title = titleValue.trim();
  const summary = summaryValue.trim();
  if (categoryId === "") return null;
  if (Array.from(title).length < 1 || Array.from(title).length > 200)
    return null;
  if (Array.from(summary).length < 1 || Array.from(summary).length > 500)
    return null;
  const tags = normalizedGuideTerms(tagsValue, 64);
  const aliases = normalizedGuideTerms(aliasesValue, 200);
  if (tags === null || aliases === null) return null;
  return { title, summary, tags, aliases, categoryId };
}

export function normalizedGuideTerms(value: string, maxScalars: number) {
  const terms = value
    .split(/\r?\n/)
    .map((term) => term.trim())
    .filter(Boolean);
  if (
    terms.length > 64 ||
    terms.some((term) => Array.from(term).length > maxScalars)
  )
    return null;
  return terms;
}

export function gitFactLabel(facts: FolderFacts) {
  if (facts.unsafeGitMarker) return "Git marker needs attention";
  if (facts.existingGitRepository && facts.nestedGitRepository) {
    return "Git repository inside another repository";
  }
  if (facts.existingGitRepository) return "Git repository detected";
  if (facts.nestedGitRepository) return "Inside an existing Git repository";
  if (!facts.gitProbeComplete)
    return "Git ancestry could not be fully inspected";
  return "No Git repository detected";
}

export function gitStatusLabel(status: GitStatusPayload | null) {
  if (status === null) return "Checking Git…";
  if (status.kind === "unavailable") return "Git unavailable";
  if (status.kind === "notRepository") return "No Git repository";
  if (status.kind === "unsafe") return "Git repository needs attention";
  if (status.kind === "recoveryRequired") return "Git operation needs recovery";
  if (status.kind === "failed")
    return status.failure ?? "Git status unavailable";
  const branch = status.detached
    ? "Detached HEAD"
    : (status.branch ?? "Unborn branch");
  if (status.conflicts > 0) return `${branch} · ${status.conflicts} conflicts`;
  const changes = status.changed + status.untracked;
  const divergence = status.divergenceKnown
    ? ` · ↑${status.ahead} ↓${status.behind}`
    : "";
  return `${branch} · ${changes === 0 ? "clean" : `${changes} changes`}${divergence}`;
}

export function isGitStatusPayload(value: unknown): value is GitStatusPayload {
  if (typeof value !== "object" || value === null) return false;
  const status = value as Partial<Record<keyof GitStatusPayload, unknown>> &
    Record<string, unknown>;
  if (
    Object.keys(status).length !== 14 ||
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
    typeof status.divergenceKnown !== "boolean"
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

function isUint32(value: unknown) {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 0 &&
    value <= 0xffff_ffff
  );
}

function isUnsafeDisplayCodePoint(code: number) {
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

function containsUnsafeDisplayCodePoint(value: string) {
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

export const isGitSyncRecoveryResultPayload = isGitSyncApplyResultPayload;

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

export async function requestGitPush(
  planToken: string,
  invoke: NativeInvoker,
): Promise<GitPushResultPayload> {
  const result = await invoke("git_push_apply_open_workspace", { planToken });
  if (!isGitPushResultPayload(result))
    throw new Error("invalid Git push result");
  return result;
}

export async function requestGitStatus(
  invoke: NativeInvoker,
): Promise<GitStatusPayload> {
  const status = await invoke("git_status_open_workspace");
  if (!isGitStatusPayload(status))
    throw new Error("invalid Git status payload");
  return status;
}

export function focusGitSyncAction(
  target: () => { focus(): void } | null,
  schedule: (callback: () => void) => unknown = (callback) =>
    requestAnimationFrame(callback),
) {
  schedule(() => target()?.focus());
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

function isBoundedText(value: unknown, minimum: number, maximum: number) {
  if (
    typeof value !== "string" ||
    value.length < minimum ||
    value.length > maximum * 2
  )
    return false;
  const length = Array.from(value).length;
  return length >= minimum && length <= maximum;
}

function isWorkspacePayload(value: unknown): value is WorkspacePayload {
  if (typeof value !== "object" || value === null) return false;
  const workspace = value as Partial<WorkspacePayload> &
    Record<string, unknown>;
  if (
    Object.keys(workspace).length !== 8 ||
    !isBoundedText(workspace.id, 1, 64) ||
    !isBoundedText(workspace.name, 1, 200) ||
    !Array.isArray(workspace.categories) ||
    workspace.categories.length > 10_000 ||
    !Array.isArray(workspace.guides) ||
    workspace.guides.length > 10_000 ||
    !Number.isSafeInteger(workspace.issueCount) ||
    (workspace.issueCount as number) < 0 ||
    (workspace.issueCount as number) > 10_000 ||
    (workspace.watchStatus !== "active" &&
      workspace.watchStatus !== "degraded") ||
    typeof workspace.watchSession !== "string" ||
    !/^[1-9][0-9]{0,19}$/.test(workspace.watchSession)
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
      Object.keys(guide).length === 11 &&
      isBoundedText(guide.id, 1, 64) &&
      isBoundedText(guide.title, 1, 200) &&
      isBoundedText(guide.summary, 1, 500) &&
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

export function focusGitCommitReview(
  target: () => { focus(): void } | null,
  schedule: (callback: () => void) => unknown = (callback) =>
    requestAnimationFrame(callback),
) {
  schedule(() => target()?.focus());
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

function folderFactSuffix(facts: FolderFacts) {
  const access = facts.writable
    ? "The folder is writable."
    : "The folder is read-only.";
  return `${access} ${gitFactLabel(facts)}.`;
}

export function nextResultIndex(
  current: number,
  length: number,
  direction: 1 | -1,
) {
  if (length === 0) return -1;
  return (current + direction + length) % length;
}

function initialParameter(name: string) {
  if (typeof window === "undefined") return null;
  return new URLSearchParams(window.location.search).get(name);
}

function clearGuideHash() {
  if (window.location.hash !== "#guide-content") return;
  window.history.replaceState(
    window.history.state,
    "",
    `${window.location.pathname}${window.location.search}`,
  );
}

function Status({ value }: { value: GuideSummary["status"] }) {
  return (
    <span className={`status status-${value.toLowerCase()}`}>
      <span aria-hidden="true" className="status-dot" />
      {value}
    </span>
  );
}

const SEARCH_MATCH_LABELS: Record<SearchMatchField, string> = {
  title: "Title",
  alias: "Alias",
  command: "Command",
  path: "Path",
  error: "Error",
  tag: "Tag",
  category: "Category",
  summary: "Summary",
  heading: "Heading",
  body: "Content",
};

function SearchExcerpt({ hit }: { hit: SearchHitPayload }) {
  return (
    <span className="result-excerpt">
      <span className="result-match-label">
        {SEARCH_MATCH_LABELS[hit.excerptField]}
      </span>
      <span>
        {highlightedExcerptSegments(hit).map((segment) =>
          segment.highlighted ? (
            <mark key={`${segment.start}-${segment.end}-mark`}>
              {segment.text}
            </mark>
          ) : (
            <span key={`${segment.start}-${segment.end}-text`}>
              {segment.text}
            </span>
          ),
        )}
      </span>
    </span>
  );
}

export function App() {
  const [query, setQuery] = useState(() => initialParameter("q") ?? "");
  const [searchScope, setSearchScope] = useState<SearchScope>(() => {
    const scope = initialParameter("scope");
    return isSearchScope(scope) ? scope : "active";
  });
  const [workspace, setWorkspace] = useState<WorkspacePayload | null>(null);
  const [gitStatus, setGitStatus] = useState<GitStatusPayload | null>(null);
  const [initializingGit, setInitializingGit] = useState(false);
  const [gitInitError, setGitInitError] = useState("");
  const [connectingRemote, setConnectingRemote] = useState(false);
  const [remoteFormOpen, setRemoteFormOpen] = useState(false);
  const [remoteName, setRemoteName] = useState("origin");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [remoteError, setRemoteError] = useState("");
  const [commitFormOpen, setCommitFormOpen] = useState(false);
  const [commitMessage, setCommitMessage] = useState("");
  const [commitPlan, setCommitPlan] = useState<GitCommitPlanPayload | null>(
    null,
  );
  const [commitStage, setCommitStage] = useState<GitCommitStage>("idle");
  const [commitError, setCommitError] = useState("");
  const [syncPanelOpen, setSyncPanelOpen] = useState(false);
  const [syncInspection, setSyncInspection] =
    useState<GitSyncInspectionPayload | null>(null);
  const [syncStage, setSyncStage] = useState<GitSyncStage>("idle");
  const [syncError, setSyncError] = useState("");
  const [selectedId, setSelectedId] = useState(
    () => initialParameter("guide") ?? "",
  );
  const [readerOpen, setReaderOpen] = useState(
    () => initialParameter("guide") !== null,
  );
  const [railOpen, setRailOpen] = useState(
    () => initialParameter("rail") === "open",
  );
  const [announcement, setAnnouncement] = useState("");
  const [openingWorkspace, setOpeningWorkspace] = useState(false);
  const [refreshingWorkspace, setRefreshingWorkspace] = useState(false);
  const [externalChangeWorkspaceId, setExternalChangeWorkspaceId] =
    useState("");
  const [watchDegradedWorkspaceId, setWatchDegradedWorkspaceId] = useState("");
  const [workspaceEventsUnavailable, setWorkspaceEventsUnavailable] =
    useState(false);
  const [pendingWorkspace, setPendingWorkspace] = useState<FolderFacts | null>(
    null,
  );
  const [workspaceName, setWorkspaceName] = useState("");
  const [initializingWorkspace, setInitializingWorkspace] = useState(false);
  const [workspaceSetupError, setWorkspaceSetupError] = useState("");
  const [gitCloneStage, setGitCloneStage] = useState<GitCloneStage>("idle");
  const [gitCloneUrl, setGitCloneUrl] = useState("");
  const [gitCloneHost, setGitCloneHost] = useState("");
  const [creatingCategory, setCreatingCategory] = useState(false);
  const [categoryName, setCategoryName] = useState("");
  const [categorySummary, setCategorySummary] = useState("");
  const [savingCategory, setSavingCategory] = useState(false);
  const [categoryError, setCategoryError] = useState("");
  const [renamingCategoryId, setRenamingCategoryId] = useState("");
  const [categoryRenameValue, setCategoryRenameValue] = useState("");
  const [categoryRenameSummary, setCategoryRenameSummary] = useState("");
  const [savingCategoryRename, setSavingCategoryRename] = useState(false);
  const [categoryRenameError, setCategoryRenameError] = useState("");
  const [updatingCategoryStatusId, setUpdatingCategoryStatusId] = useState("");
  const [categoryStatusError, setCategoryStatusError] = useState<{
    id: string;
    message: string;
  } | null>(null);
  const [reorderingCategoryId, setReorderingCategoryId] = useState("");
  const [removingCategoryId, setRemovingCategoryId] = useState("");
  const [categoryReorderError, setCategoryReorderError] = useState<{
    id: string;
    message: string;
  } | null>(null);
  const [creatingGuide, setCreatingGuide] = useState(false);
  const [guideTitle, setGuideTitle] = useState("");
  const [guideSummary, setGuideSummary] = useState("");
  const [guideCategoryId, setGuideCategoryId] = useState("");
  const [savingNewGuide, setSavingNewGuide] = useState(false);
  const [guideCreateError, setGuideCreateError] = useState("");
  const [guideBody, setGuideBody] = useState<GuideBodyPayload | null>(null);
  const [editingGuideDetails, setEditingGuideDetails] = useState(false);
  const [detailTitle, setDetailTitle] = useState("");
  const [detailSummary, setDetailSummary] = useState("");
  const [detailCategoryId, setDetailCategoryId] = useState("");
  const [detailTags, setDetailTags] = useState("");
  const [detailAliases, setDetailAliases] = useState("");
  const [savingGuideDetails, setSavingGuideDetails] = useState(false);
  const [guideDetailsError, setGuideDetailsError] = useState("");
  const [editingGuide, setEditingGuide] = useState(false);
  const [draftBody, setDraftBody] = useState("");
  const [savingGuide, setSavingGuide] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [updatingGuideStatus, setUpdatingGuideStatus] = useState(false);
  const [verifyingGuide, setVerifyingGuide] = useState(false);
  const [importingAttachment, setImportingAttachment] = useState(false);
  const [removingAttachmentId, setRemovingAttachmentId] = useState("");
  const [removingGuide, setRemovingGuide] = useState(false);
  const [guideStatusError, setGuideStatusError] = useState("");
  const [searchHits, setSearchHits] = useState<SearchHitPayload[] | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const readerRef = useRef<HTMLElement>(null);
  const resultListRef = useRef<HTMLUListElement>(null);
  const lastResultRef = useRef<HTMLButtonElement | null>(null);
  const railToggleRef = useRef<HTMLButtonElement>(null);
  const workspaceRefreshRef = useRef<HTMLButtonElement>(null);
  const remoteButtonRef = useRef<HTMLButtonElement>(null);
  const remoteNameRef = useRef<HTMLInputElement>(null);
  const remoteErrorRef = useRef<HTMLParagraphElement>(null);
  const commitButtonRef = useRef<HTMLButtonElement>(null);
  const commitMessageRef = useRef<HTMLInputElement>(null);
  const commitApplyRef = useRef<HTMLButtonElement>(null);
  const commitErrorRef = useRef<HTMLParagraphElement>(null);
  const syncButtonRef = useRef<HTMLButtonElement>(null);
  const syncPrimaryRef = useRef<HTMLButtonElement>(null);
  const syncErrorRef = useRef<HTMLParagraphElement>(null);
  const syncGenerationRef = useRef(0);
  const editButtonRef = useRef<HTMLButtonElement>(null);
  const editDetailsButtonRef = useRef<HTMLButtonElement>(null);
  const guideStatusButtonRef = useRef<HTMLButtonElement>(null);
  const guideVerifyButtonRef = useRef<HTMLButtonElement>(null);
  const attachmentImportButtonRef = useRef<HTMLButtonElement>(null);
  const attachmentRemoveButtonRefs = useRef(
    new Map<string, HTMLButtonElement>(),
  );
  const guideRemoveButtonRef = useRef<HTMLButtonElement>(null);
  const guideStatusErrorRef = useRef<HTMLParagraphElement>(null);
  const detailTitleRef = useRef<HTMLInputElement>(null);
  const detailSummaryRef = useRef<HTMLTextAreaElement>(null);
  const detailCategoryRef = useRef<HTMLSelectElement>(null);
  const detailTagsRef = useRef<HTMLTextAreaElement>(null);
  const detailAliasesRef = useRef<HTMLTextAreaElement>(null);
  const guideDetailsErrorRef = useRef<HTMLParagraphElement>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const saveErrorRef = useRef<HTMLParagraphElement>(null);
  const workspaceNameRef = useRef<HTMLInputElement>(null);
  const workspaceSetupErrorRef = useRef<HTMLParagraphElement>(null);
  const gitCloneUrlRef = useRef<HTMLInputElement>(null);
  const gitCloneCompleteRef = useRef<HTMLButtonElement>(null);
  const gitCloneValidationGenerationRef = useRef(0);
  const categoryCreateButtonRef = useRef<HTMLButtonElement>(null);
  const categoryNameRef = useRef<HTMLInputElement>(null);
  const categoryErrorRef = useRef<HTMLParagraphElement>(null);
  const categoryRenameInputRef = useRef<HTMLInputElement>(null);
  const categoryRenameErrorRef = useRef<HTMLParagraphElement>(null);
  const categoryStatusErrorRef = useRef<HTMLParagraphElement>(null);
  const categoryReorderErrorRef = useRef<HTMLParagraphElement>(null);
  const categoryRenameButtonRefs = useRef(new Map<string, HTMLButtonElement>());
  const guideCreateButtonRef = useRef<HTMLButtonElement>(null);
  const guideTitleRef = useRef<HTMLInputElement>(null);
  const guideCreateErrorRef = useRef<HTMLParagraphElement>(null);
  const workspaceIdRef = useRef("");
  const workspaceSessionRef = useRef("");
  const editGenerationRef = useRef(0);
  const guides = workspace?.guides ?? [];
  const scopedGuides = useMemo(
    () =>
      guides.filter((guide) =>
        guideMatchesSearchScope(guide.status, searchScope),
      ),
    [guides, searchScope],
  );
  const visibleGuides = useMemo(
    () => rankedGuides(scopedGuides, query, searchHits),
    [scopedGuides, query, searchHits],
  );
  const searchHitsByGuideId = useMemo(
    () => new Map((searchHits ?? []).map((hit) => [hit.guideId, hit])),
    [searchHits],
  );
  const selectedById = guides.find((guide) => guide.id === selectedId) ?? null;
  const selectedIsVisible = visibleGuides.some(
    (guide) => guide.id === selectedId,
  );
  const dirtyGuide =
    editingGuide &&
    guideBody !== null &&
    guideBody.id === selectedById?.id &&
    hasUnsavedChanges(guideBody.body, draftBody);
  const dirtyGuideDetails =
    editingGuideDetails &&
    selectedById !== null &&
    (selectedById.title !== detailTitle ||
      selectedById.summary !== detailSummary ||
      selectedById.categoryId !== detailCategoryId ||
      selectedById.tags.join("\n") !== detailTags ||
      selectedById.aliases.join("\n") !== detailAliases);
  const selected =
    selectedIsVisible || dirtyGuide || dirtyGuideDetails
      ? selectedById
      : (visibleGuides[0] ?? null);
  const categoryBeingRenamed =
    workspace?.categories.find(
      (category) => category.id === renamingCategoryId,
    ) ?? null;
  const dirtyCategoryRename =
    categoryBeingRenamed !== null &&
    (categoryRenameValue !== categoryBeingRenamed.name ||
      categoryRenameSummary !== (categoryBeingRenamed.summary ?? ""));
  const dirtyCategoryCreate =
    creatingCategory &&
    (categoryName.trim() !== "" || categorySummary.trim() !== "");
  const dirtyGuideCreate =
    creatingGuide && (guideTitle.trim() !== "" || guideSummary.trim() !== "");
  const workspaceOperationBusy =
    savingGuide ||
    savingNewGuide ||
    savingGuideDetails ||
    savingCategory ||
    savingCategoryRename ||
    updatingGuideStatus ||
    verifyingGuide ||
    importingAttachment ||
    removingAttachmentId !== "" ||
    updatingCategoryStatusId !== "" ||
    reorderingCategoryId !== "" ||
    removingCategoryId !== "" ||
    removingGuide ||
    refreshingWorkspace ||
    syncPanelOpen;
  const gitCloneOperationBusy =
    gitCloneStage === "validating" ||
    gitCloneStage === "cancelling" ||
    gitCloneStage === "completing";
  const gitCloneRequiresResolution =
    gitCloneStage === "validated" || gitCloneStage === "recovery";
  const commitOperationBusy = commitStage !== "idle";
  const syncOperationBusy = syncStage !== "idle";
  const syncPresentation =
    syncInspection === null ? null : gitSyncPresentation(syncInspection);
  const dirtyGitCommit =
    commitFormOpen && (commitMessage.trim() !== "" || commitPlan !== null);
  const autoRefreshBlocked = automaticRefreshBlocked({
    busy:
      workspaceOperationBusy ||
      openingWorkspace ||
      gitCloneOperationBusy ||
      commitOperationBusy ||
      syncOperationBusy,
    dirtyGuide,
    dirtyGuideDetails,
    dirtyCategoryRename,
    dirtyCategoryCreate,
    dirtyGuideCreate,
    dirtyGitCommit,
  });
  const externalChangePending =
    workspace !== null && externalChangeWorkspaceId === workspace.id;
  const watchDegraded =
    workspace !== null &&
    (workspaceEventsUnavailable ||
      workspace.watchStatus === "degraded" ||
      watchDegradedWorkspaceId === workspace.id);
  workspaceIdRef.current = workspace?.id ?? "";
  workspaceSessionRef.current = workspace?.watchSession ?? "";
  function noteDraftChange() {
    editGenerationRef.current += 1;
  }
  const refreshWorkspaceFromEffect = useEffectEvent(() => {
    void refreshWorkspace(true);
  });
  const cancelGitCommitFromEffect = useEffectEvent(() => {
    void cancelGitCommit();
  });
  const cancelGitSyncFromEffect = useEffectEvent(() => {
    void cancelGitSync();
  });

  useEffect(() => {
    const theme = initialParameter("theme");
    if (theme === "light" || theme === "dark") {
      document.documentElement.setAttribute("data-theme", theme);
    }
    if (initialParameter("focus") === "search") {
      requestAnimationFrame(() => searchRef.current?.focus());
    }
    function revealGuideTarget() {
      if (window.location.hash !== "#guide-content") return;
      setReaderOpen(true);
      requestAnimationFrame(() => readerRef.current?.focus());
    }
    window.addEventListener("hashchange", revealGuideTarget);
    return () => window.removeEventListener("hashchange", revealGuideTarget);
  }, []);

  useEffect(() => {
    const next = searchStatePath(
      window.location.pathname,
      window.location.search,
      window.location.hash,
      query,
      searchScope,
    );
    window.history.replaceState(window.history.state, "", next);
  }, [query, searchScope]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<unknown>(WORKSPACE_INVALIDATED_EVENT, ({ payload }) => {
          if (
            !active ||
            !isWorkspaceWatchEvent(payload) ||
            !isCurrentWorkspaceWatchEvent(
              payload,
              workspaceIdRef.current,
              workspaceSessionRef.current,
            )
          )
            return;
          if (payload.kind === "degraded") {
            setWatchDegradedWorkspaceId(payload.workspaceId);
          }
          setExternalChangeWorkspaceId(payload.workspaceId);
        }),
      )
      .then((removeListener) => {
        if (active) unlisten = removeListener;
        else removeListener();
      })
      .catch(() => {
        if (active) setWorkspaceEventsUnavailable(true);
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let blurred = false;
    function markBlurred() {
      blurred = true;
    }
    function reconcileOnRefocus() {
      const workspaceId = workspaceIdRef.current;
      if (!blurred || workspaceId === "") return;
      blurred = false;
      setExternalChangeWorkspaceId(workspaceId);
    }
    window.addEventListener("blur", markBlurred);
    window.addEventListener("focus", reconcileOnRefocus);
    return () => {
      window.removeEventListener("blur", markBlurred);
      window.removeEventListener("focus", reconcileOnRefocus);
    };
  }, []);

  useEffect(() => {
    if (!externalChangePending) return;
    if (autoRefreshBlocked) {
      if (!workspaceOperationBusy) {
        setAnnouncement(
          "Files changed outside Rehow. Finish or discard your edits, then refresh.",
        );
      }
      return;
    }
    refreshWorkspaceFromEffect();
  }, [autoRefreshBlocked, externalChangePending, workspaceOperationBusy]);

  useEffect(() => {
    function handleShortcut(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      const editing = target?.matches(
        "input, select, textarea, [contenteditable='true']",
      );
      if (
        (event.ctrlKey || event.metaKey) &&
        event.key.toLocaleLowerCase() === "k"
      ) {
        event.preventDefault();
        searchRef.current?.focus();
      } else if (event.key === "/" && !editing) {
        event.preventDefault();
        searchRef.current?.focus();
      } else if (event.key === "Escape") {
        if (
          syncPanelOpen &&
          syncStage !== "applying" &&
          syncStage !== "recovering" &&
          syncStage !== "cancelling"
        ) {
          event.preventDefault();
          cancelGitSyncFromEffect();
          return;
        }
        if (commitFormOpen && !commitOperationBusy) {
          event.preventDefault();
          cancelGitCommitFromEffect();
          return;
        }
        if (remoteFormOpen && !connectingRemote) {
          event.preventDefault();
          setRemoteFormOpen(false);
          setRemoteError("");
          requestAnimationFrame(() => remoteButtonRef.current?.focus());
          return;
        }
        if (editingGuide || editingGuideDetails) return;
        if (railOpen) {
          setRailOpen(false);
          requestAnimationFrame(() => railToggleRef.current?.focus());
        } else if (readerOpen) {
          setReaderOpen(false);
          clearGuideHash();
          requestAnimationFrame(() => {
            (lastResultRef.current ?? searchRef.current)?.focus();
          });
        } else if (!editing) {
          searchRef.current?.focus();
        }
      }
    }
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [
    commitFormOpen,
    commitOperationBusy,
    connectingRemote,
    editingGuide,
    editingGuideDetails,
    railOpen,
    readerOpen,
    remoteFormOpen,
    syncPanelOpen,
    syncStage,
  ]);

  useEffect(() => {
    if (
      !dirtyGuide &&
      !dirtyGuideDetails &&
      !dirtyCategoryRename &&
      !dirtyCategoryCreate &&
      !dirtyGitCommit
    )
      return;
    function warnBeforeUnload(event: BeforeUnloadEvent) {
      event.preventDefault();
      event.returnValue = "";
    }
    window.addEventListener("beforeunload", warnBeforeUnload);
    return () => window.removeEventListener("beforeunload", warnBeforeUnload);
  }, [
    dirtyCategoryCreate,
    dirtyCategoryRename,
    dirtyGitCommit,
    dirtyGuide,
    dirtyGuideDetails,
  ]);

  useEffect(() => {
    if (workspace === null || selectedId === "") {
      setGuideBody(null);
      return;
    }
    let active = true;
    setGuideBody(null);
    setEditingGuide(false);
    setEditingGuideDetails(false);
    setSaveError("");
    setGuideDetailsError("");
    setGuideStatusError("");
    import("@tauri-apps/api/core")
      .then(({ invoke }) =>
        invoke<GuideBodyPayload>("read_open_guide", { guideId: selectedId }),
      )
      .then((payload) => {
        if (active) {
          setGuideBody(payload);
          setDraftBody(payload.body);
        }
      })
      .catch(() => {
        if (active) {
          setAnnouncement(
            "This Guide could not be read from the open Workspace.",
          );
        }
      });
    return () => {
      active = false;
    };
  }, [selectedId, workspace]);

  useEffect(() => {
    if (workspace === null || query.trim() === "") {
      setSearchHits(null);
      return;
    }
    let active = true;
    setSearchHits(null);
    const timeout = window.setTimeout(() => {
      import("@tauri-apps/api/core")
        .then(({ invoke }) =>
          invoke<unknown>("search_open_workspace", {
            query,
            scope: searchScope,
          }),
        )
        .then((payload) => {
          if (!Array.isArray(payload) || !payload.every(isSearchHitPayload)) {
            throw new Error("invalid search payload");
          }
          if (active) setSearchHits(payload);
        })
        .catch(() => {
          if (active) {
            setSearchHits([]);
            setAnnouncement("Search could not be completed.");
          }
        });
    }, 20);
    return () => {
      active = false;
      window.clearTimeout(timeout);
    };
  }, [query, searchScope, workspace]);

  useEffect(() => {
    if (workspace === null) {
      setGitStatus(null);
      return;
    }
    let active = true;
    setGitStatus(null);
    import("@tauri-apps/api/core")
      .then(({ invoke }) =>
        requestGitStatus((command, arguments_) => invoke(command, arguments_)),
      )
      .then((status) => {
        if (active) setGitStatus(status);
      })
      .catch(() => {
        if (active) {
          setGitStatus({
            kind: "failed",
            version: null,
            branch: null,
            detached: false,
            upstream: null,
            divergenceKnown: false,
            ahead: 0,
            behind: 0,
            changed: 0,
            untracked: 0,
            conflicts: 0,
            remotes: [],
            remoteCount: 0,
            failure: "Git status unavailable",
          });
        }
      });
    return () => {
      active = false;
    };
  }, [workspace]);

  useEffect(() => {
    if (dirtyGuide || dirtyGuideDetails) return;
    if (selectedId !== "" && selectedIsVisible) return;
    const nextId = visibleGuides[0]?.id ?? "";
    if (nextId !== selectedId) setSelectedId(nextId);
  }, [
    dirtyGuide,
    dirtyGuideDetails,
    selectedId,
    selectedIsVisible,
    visibleGuides,
  ]);

  function confirmDiscard() {
    if (workspaceOperationBusy) return false;
    if (dirtyGuide && !window.confirm("Discard your unsaved Guide changes?")) {
      return false;
    }
    if (
      dirtyGuideDetails &&
      !window.confirm("Discard your unsaved Guide detail changes?")
    ) {
      return false;
    }
    if (
      dirtyCategoryRename &&
      !window.confirm("Discard this Category name change?")
    ) {
      return false;
    }
    if (
      dirtyCategoryCreate &&
      !window.confirm("Discard this unfinished Category?")
    ) {
      return false;
    }
    return (
      !dirtyGuideCreate ||
      window.confirm("Discard this unfinished Guide draft?")
    );
  }

  function openGuide(id: string, trigger?: HTMLButtonElement) {
    if (id !== selectedId && !confirmDiscard()) return;
    if (trigger) lastResultRef.current = trigger;
    setSelectedId(id);
    setReaderOpen(true);
    requestAnimationFrame(() => readerRef.current?.focus());
  }

  function changeSearchScope(nextScope: SearchScope) {
    if (nextScope === searchScope) return;
    if (
      selectedById !== null &&
      !guideMatchesSearchScope(selectedById.status, nextScope)
    ) {
      if (!confirmDiscard()) return;
      setEditingGuide(false);
      setEditingGuideDetails(false);
      setSaveError("");
      setGuideDetailsError("");
      setSelectedId("");
    }
    setSearchScope(nextScope);
  }

  function closeReader() {
    if (!confirmDiscard()) return;
    setReaderOpen(false);
    clearGuideHash();
    requestAnimationFrame(() => {
      (lastResultRef.current ?? searchRef.current)?.focus();
    });
  }

  function handleResultKey(
    event: React.KeyboardEvent<HTMLButtonElement>,
    index: number,
  ) {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const next = nextResultIndex(
      index,
      visibleGuides.length,
      event.key === "ArrowDown" ? 1 : -1,
    );
    const guide = visibleGuides[next];
    if (!guide) return;
    if (guide.id !== selectedId && !confirmDiscard()) return;
    setSelectedId(guide.id);
    resultListRef.current
      ?.querySelectorAll<HTMLButtonElement>(".result-row")
      .item(next)
      .focus();
  }

  function resetGitCloneUi(stage: GitCloneStage = "idle") {
    setGitCloneStage(stage);
    setGitCloneUrl("");
    setGitCloneHost("");
  }

  function resetGitSyncUi() {
    syncGenerationRef.current += 1;
    setSyncPanelOpen(false);
    setSyncInspection(null);
    setSyncStage("idle");
    setSyncError("");
  }

  function installClonedWorkspace(completed: WorkspacePayload) {
    setWorkspace(completed);
    setGitStatus(null);
    setPendingWorkspace(null);
    setSelectedId(completed.guides[0]?.id ?? "");
    setReaderOpen(false);
    setQuery("");
    setExternalChangeWorkspaceId("");
    setWatchDegradedWorkspaceId(
      completed.watchStatus === "degraded" ? completed.id : "",
    );
    resetGitCloneUi();
    setAnnouncement(
      `${completed.name} was cloned and opened with ${resultCountLabel(completed.guides.length)}.`,
    );
    requestAnimationFrame(() => searchRef.current?.focus());
  }

  async function openWorkspace() {
    if (
      gitCloneOperationBusy ||
      gitCloneRequiresResolution ||
      syncOperationBusy ||
      !confirmDiscard()
    )
      return;
    setWorkspaceSetupError("");
    setOpeningWorkspace(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<OpenWorkspaceResult>("choose_workspace");
      if (result.kind !== "cancelled") {
        resetGitCloneUi();
        resetGitSyncUi();
        setGitInitError("");
        setExternalChangeWorkspaceId("");
        setWatchDegradedWorkspaceId("");
        setCreatingCategory(false);
        setCategoryName("");
        setCategorySummary("");
        setCategoryError("");
        setRenamingCategoryId("");
        setCategoryRenameValue("");
        setCategoryRenameError("");
        setCategoryStatusError(null);
        setCreatingGuide(false);
        setGuideTitle("");
        setGuideSummary("");
        setGuideCategoryId("");
        setGuideCreateError("");
      }
      if (result.kind === "compatible") {
        setWorkspace(result.workspace);
        setPendingWorkspace(null);
        setWorkspaceSetupError("");
        setSelectedId(result.workspace.guides[0]?.id ?? "");
        setReaderOpen(false);
        setQuery("");
        setAnnouncement(
          `${result.workspace.name} opened with ${resultCountLabel(result.workspace.guides.length)}. ${folderFactSuffix(result.workspace.facts)}`,
        );
        requestAnimationFrame(() => searchRef.current?.focus());
      } else if (result.kind === "empty") {
        setWorkspace(null);
        setPendingWorkspace(result.facts);
        setWorkspaceName("");
        setWorkspaceSetupError("");
        setSelectedId("");
        setReaderOpen(false);
        setAnnouncement(
          `Empty folder selected. Create a new Workspace or clone an existing one. ${folderFactSuffix(result.facts)}`,
        );
        requestAnimationFrame(() => workspaceNameRef.current?.focus());
      } else if (result.kind !== "cancelled") {
        setPendingWorkspace(null);
        const messages: Record<
          Exclude<
            OpenWorkspaceResult["kind"],
            "compatible" | "cancelled" | "empty"
          >,
          string
        > = {
          newerUnsupported:
            "That Workspace was created by a newer Rehow version.",
          ordinaryMarkdown:
            "That folder is a Markdown collection. Safe import is coming next.",
          unrelatedNonempty: "That nonempty folder is not a Rehow Workspace.",
          malformed: "That Workspace is malformed and was not opened.",
        };
        setAnnouncement(
          `${messages[result.kind]} ${folderFactSuffix(result.facts)}`,
        );
      }
    } catch {
      setAnnouncement("The native Workspace picker is unavailable.");
    } finally {
      setOpeningWorkspace(false);
    }
  }

  async function refreshWorkspace(automatic = false) {
    if (workspace === null || refreshingWorkspace) return;
    if (automatic ? autoRefreshBlocked : !confirmDiscard()) return;
    const previousSelectedId = selectedId;
    const previousSelectedTitle = selected?.title ?? "The selected Guide";
    const startedEditGeneration = editGenerationRef.current;
    setExternalChangeWorkspaceId("");
    setRefreshingWorkspace(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const refreshed = await invoke<WorkspacePayload>(
        "refresh_open_workspace",
      );
      if (
        !refreshResultMayApply(startedEditGeneration, editGenerationRef.current)
      ) {
        setExternalChangeWorkspaceId(workspace.id);
        setAnnouncement(
          "Workspace files changed, but edits made during refresh were kept. Refresh again when ready.",
        );
        return;
      }
      setWatchDegradedWorkspaceId(
        refreshed.watchStatus === "degraded" ? refreshed.id : "",
      );
      const selectedRemoved =
        previousSelectedId !== "" &&
        !refreshed.guides.some(({ id }) => id === previousSelectedId);
      const nextGuideId = refreshedGuideId(
        previousSelectedId,
        refreshed.guides,
      );
      setWorkspace(refreshed);
      setSelectedId(nextGuideId);
      setEditingGuide(false);
      setEditingGuideDetails(false);
      setCreatingGuide(false);
      setCreatingCategory(false);
      setRenamingCategoryId("");
      setGuideCreateError("");
      setCategoryError("");
      setCategoryRenameError("");
      if (selectedRemoved) {
        setReaderOpen(false);
        setAnnouncement(
          `${previousSelectedTitle} was removed or is unavailable. Workspace ${automatic ? "updated automatically" : "refreshed"} with ${resultCountLabel(refreshed.guides.length)}.`,
        );
        if (!automatic) requestAnimationFrame(() => searchRef.current?.focus());
      } else {
        const issueMessage =
          refreshed.issueCount === 0
            ? "No issues found."
            : `${refreshed.issueCount} item${refreshed.issueCount === 1 ? "" : "s"} need attention.`;
        setAnnouncement(
          `Workspace ${automatic ? "updated automatically" : "refreshed"} with ${resultCountLabel(refreshed.guides.length)}. ${issueMessage}`,
        );
        if (!automatic) {
          requestAnimationFrame(() => workspaceRefreshRef.current?.focus());
        }
      }
    } catch (error) {
      const failure = nativeCommandError(error);
      setAnnouncement(failure.message);
      if (automatic) setWatchDegradedWorkspaceId(workspace.id);
      else requestAnimationFrame(() => workspaceRefreshRef.current?.focus());
    } finally {
      setRefreshingWorkspace(false);
    }
  }

  async function createWorkspace() {
    const name = normalizedWorkspaceName(workspaceName);
    if (name === null || initializingWorkspace) return;
    setWorkspaceName(name);
    setInitializingWorkspace(true);
    setWorkspaceSetupError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const created = await invoke<WorkspacePayload>(
        "initialize_selected_workspace",
        { name },
      );
      setWorkspace(created);
      setGitInitError("");
      setExternalChangeWorkspaceId("");
      setWatchDegradedWorkspaceId(
        created.watchStatus === "degraded" ? created.id : "",
      );
      setPendingWorkspace(null);
      setSelectedId("");
      setQuery("");
      setAnnouncement(`${created.name} was created and opened.`);
      requestAnimationFrame(() => searchRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setWorkspaceSetupError(failure.message);
      if (failure.code !== "invalidName" && failure.code !== "busy") {
        setPendingWorkspace(null);
      }
      setAnnouncement(failure.message);
      requestAnimationFrame(() => workspaceSetupErrorRef.current?.focus());
    } finally {
      setInitializingWorkspace(false);
    }
  }

  function beginGitClone() {
    if (pendingWorkspace?.writable !== true || initializingWorkspace) return;
    setWorkspaceSetupError("");
    setGitCloneStage("entry");
    setGitCloneHost("");
    setAnnouncement(
      "Enter an HTTPS or SSH repository address to validate a Git Workspace.",
    );
    requestAnimationFrame(() => gitCloneUrlRef.current?.focus());
  }

  function returnToWorkspaceCreation() {
    if (gitCloneOperationBusy) return;
    resetGitCloneUi();
    setWorkspaceSetupError("");
    setAnnouncement("Create a new Workspace in the selected empty folder.");
    requestAnimationFrame(() => workspaceNameRef.current?.focus());
  }

  async function validateGitClone(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pendingWorkspace?.writable !== true || gitCloneOperationBusy) return;
    const remoteUrl = normalizedCloneUrl(gitCloneUrl);
    if (remoteUrl === null) {
      setWorkspaceSetupError(
        "Enter a canonical HTTPS or SSH repository address without a password, query text, or fragment.",
      );
      requestAnimationFrame(() => workspaceSetupErrorRef.current?.focus());
      return;
    }
    setGitCloneUrl(remoteUrl);
    setGitCloneHost("");
    setWorkspaceSetupError("");
    setGitCloneStage("validating");
    const validationGeneration = ++gitCloneValidationGenerationRef.current;
    setAnnouncement(
      "Waiting for confirmation to validate the Git Workspace. The selected folder will not change.",
    );
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const plan = await invoke<unknown>("git_clone_plan_selected_workspace", {
        remoteUrl,
      });
      if (!isGitClonePlanPayload(plan))
        throw new Error("invalid Git clone plan");
      setGitCloneHost(plan.host);
      const result = await invoke<unknown>(
        "git_clone_apply_selected_workspace",
        {
          planToken: plan.planToken,
        },
      );
      if (validationGeneration !== gitCloneValidationGenerationRef.current)
        return;
      gitCloneValidationGenerationRef.current += 1;
      if (!isGitCloneApplyResultPayload(result)) {
        setGitCloneStage("recovery");
        setWorkspaceSetupError(
          "Rehow could not confirm the validation state. Complete the clone to reconcile it, or cancel this Workspace setup.",
        );
        setAnnouncement(
          "Git Workspace validation needs reconciliation before another setup action.",
        );
        requestAnimationFrame(() => gitCloneCompleteRef.current?.focus());
        return;
      }
      if (!result.validated) {
        setGitCloneStage("entry");
        setAnnouncement(
          "Git Workspace validation was declined. The selected folder was not changed.",
        );
        requestAnimationFrame(() => gitCloneUrlRef.current?.focus());
        return;
      }
      setGitCloneStage("validated");
      setAnnouncement(
        `Git Workspace from ${plan.host} was validated. Complete the clone to write it to the selected folder.`,
      );
      requestAnimationFrame(() => gitCloneCompleteRef.current?.focus());
    } catch (error) {
      if (validationGeneration !== gitCloneValidationGenerationRef.current)
        return;
      gitCloneValidationGenerationRef.current += 1;
      const failure = nativeCommandError(error);
      setGitCloneStage("entry");
      setWorkspaceSetupError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => workspaceSetupErrorRef.current?.focus());
    }
  }

  async function cancelGitCloneValidation() {
    if (gitCloneStage !== "validating") return;
    const validationGeneration = gitCloneValidationGenerationRef.current;
    setGitCloneStage("cancelling");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<unknown>("git_clone_cancel_active");
      if (!isGitCloneCancelResultPayload(result))
        throw new Error("invalid Git clone cancellation result");
      if (validationGeneration !== gitCloneValidationGenerationRef.current)
        return;
      if (result.cancellationRequested) {
        setAnnouncement("Cancelling Git Workspace validation…");
      } else {
        setGitCloneStage("validating");
        setAnnouncement("Validation is finishing; wait for the result.");
      }
    } catch (error) {
      if (validationGeneration !== gitCloneValidationGenerationRef.current)
        return;
      const failure = nativeCommandError(error);
      setGitCloneStage("validating");
      setWorkspaceSetupError(failure.message);
      setAnnouncement(failure.message);
    }
  }

  async function completeGitClone() {
    if (gitCloneStage !== "validated" && gitCloneStage !== "recovery") return;
    const retryStage = gitCloneStage;
    setGitCloneStage("completing");
    setWorkspaceSetupError("");
    setAnnouncement(
      "Waiting for confirmation to complete the validated Git clone.",
    );
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<unknown>(
        "git_clone_complete_selected_workspace",
      );
      if (!isGitCloneCompleteResultPayload(result)) {
        try {
          const refreshed = await invoke<unknown>("refresh_open_workspace");
          if (isWorkspacePayload(refreshed)) {
            installClonedWorkspace(refreshed);
            return;
          }
        } catch {
          // A missing active Workspace means native completion did not publish
          // a state the renderer can safely adopt.
        }
        setGitCloneStage("recovery");
        setWorkspaceSetupError(
          "Rehow could not reconcile clone completion. Retry completion or cancel setup; the selected folder will not be overwritten.",
        );
        setAnnouncement(
          "Git clone completion needs reconciliation before another setup action.",
        );
        requestAnimationFrame(() => gitCloneCompleteRef.current?.focus());
        return;
      }
      if (result.kind === "declined") {
        setGitCloneStage(retryStage);
        setAnnouncement(
          "Git clone completion was declined. The validated Workspace is ready when you are.",
        );
        requestAnimationFrame(() => gitCloneCompleteRef.current?.focus());
        return;
      }
      installClonedWorkspace(result.workspace);
    } catch (error) {
      const failure = nativeCommandError(error);
      setGitCloneStage(retryStage);
      setWorkspaceSetupError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => workspaceSetupErrorRef.current?.focus());
    }
  }

  async function initializeGit() {
    if (
      workspace === null ||
      gitStatus?.kind !== "notRepository" ||
      initializingGit
    )
      return;
    setInitializingGit(true);
    setGitInitError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const plan = await invoke<unknown>("git_initialize_plan_open_workspace");
      if (!isGitInitPlanPayload(plan))
        throw new Error("invalid Git initialization plan");
      const result = await invoke<unknown>(
        "git_initialize_apply_open_workspace",
        {
          planToken: plan.planToken,
        },
      );
      if (!isGitInitResultPayload(result))
        throw new Error("invalid Git initialization result");
      if (!result.initialized) {
        setAnnouncement("Git initialization cancelled.");
        return;
      }
      setGitStatus(null);
      setWorkspace((current) =>
        current === null
          ? null
          : {
              ...current,
              facts: {
                ...current.facts,
                existingGitRepository: true,
                gitProbeComplete: true,
                unsafeGitMarker: false,
              },
            },
      );
      setAnnouncement("Git initialized with initial branch main.");
      requestAnimationFrame(() => searchRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setGitInitError(failure.message);
      setAnnouncement(failure.message);
    } finally {
      setInitializingGit(false);
    }
  }

  function beginRemoteConnection() {
    if (
      workspace === null ||
      gitStatus?.kind !== "ready" ||
      commitFormOpen ||
      commitOperationBusy ||
      syncPanelOpen ||
      syncOperationBusy
    )
      return;
    setRemoteName("origin");
    setRemoteUrl("");
    setRemoteError("");
    setRemoteFormOpen(true);
    requestAnimationFrame(() => remoteNameRef.current?.focus());
  }

  function cancelRemoteConnection() {
    if (connectingRemote) return;
    setRemoteFormOpen(false);
    setRemoteError("");
    requestAnimationFrame(() => remoteButtonRef.current?.focus());
  }

  async function connectRemote(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (workspace === null || connectingRemote) return;
    const name = normalizedRemoteName(remoteName);
    const url = normalizedRemoteUrl(remoteUrl);
    if (name === null || url === null) {
      setRemoteError(
        name === null
          ? "Use a lowercase name with letters, numbers, dots, dashes, or underscores."
          : "Enter an HTTPS or SSH repository address.",
      );
      requestAnimationFrame(() => remoteErrorRef.current?.focus());
      return;
    }
    setRemoteName(name);
    setRemoteUrl(url);
    setRemoteError("");
    setConnectingRemote(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const plan = await invoke<unknown>("git_remote_plan_open_workspace", {
        remoteName: name,
        remoteUrl: url,
      });
      if (!isGitRemotePlanPayload(plan))
        throw new Error("invalid Git remote plan");
      const result = await invoke<unknown>("git_remote_apply_open_workspace", {
        planToken: plan.planToken,
      });
      if (!isGitRemoteResultPayload(result))
        throw new Error("invalid Git remote result");
      if (!result.connected) {
        setAnnouncement("Git remote connection cancelled.");
        return;
      }
      setRemoteFormOpen(false);
      setRemoteUrl("");
      setWorkspace((current) => (current === null ? null : { ...current }));
      setAnnouncement(
        `Git remote ${plan.remoteName} was added for ${plan.host}. No network connection was made.`,
      );
      requestAnimationFrame(() => searchRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setRemoteError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => remoteErrorRef.current?.focus());
    } finally {
      setConnectingRemote(false);
    }
  }

  function beginGitCommit() {
    if (
      workspace === null ||
      gitStatus?.kind !== "ready" ||
      remoteFormOpen ||
      syncPanelOpen ||
      syncOperationBusy ||
      commitOperationBusy ||
      workspaceOperationBusy ||
      dirtyGuide ||
      dirtyGuideDetails ||
      dirtyCategoryCreate ||
      dirtyCategoryRename ||
      dirtyGuideCreate
    )
      return;
    setCommitMessage("");
    setCommitPlan(null);
    setCommitError("");
    setCommitFormOpen(true);
    requestAnimationFrame(() => commitMessageRef.current?.focus());
  }

  function resetGitCommitUi() {
    setCommitFormOpen(false);
    setCommitMessage("");
    setCommitPlan(null);
    setCommitError("");
    setCommitStage("idle");
  }

  async function cancelGitCommit() {
    if (commitOperationBusy) return;
    if (commitPlan === null) {
      resetGitCommitUi();
      requestAnimationFrame(() => commitButtonRef.current?.focus());
      return;
    }
    setCommitStage("cancelling");
    setCommitError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<unknown>("git_commit_cancel_open_workspace", {
        planToken: commitPlan.planToken,
      });
      if (!isGitCommitCancelResultPayload(result))
        throw new Error("invalid Git commit cancellation result");
      resetGitCommitUi();
      setAnnouncement("Git commit plan cancelled.");
      requestAnimationFrame(() => commitButtonRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setCommitStage("idle");
      setCommitError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => commitErrorRef.current?.focus());
    }
  }

  async function planGitCommit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (workspace === null || commitOperationBusy || commitPlan !== null)
      return;
    const message = normalizedGitCommitMessage(commitMessage);
    if (message === null) {
      setCommitError("Enter a one-line message between 1 and 200 characters.");
      requestAnimationFrame(() => commitErrorRef.current?.focus());
      return;
    }
    setCommitMessage(message);
    setCommitError("");
    setCommitStage("planning");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const plan = await invoke<unknown>("git_commit_plan_open_workspace", {
        message,
      });
      if (!isGitCommitPlanPayload(plan))
        throw new Error("invalid Git commit plan");
      setCommitPlan(plan);
      focusGitCommitReview(() => commitApplyRef.current);
      setAnnouncement(
        `Commit plan ready for branch ${plan.branch}. Review the managed change counts.`,
      );
    } catch (error) {
      const failure = nativeCommandError(error);
      setCommitError(failure.message);
      setAnnouncement(failure.message);
      if (failure.code === "recoveryRequired") {
        setWorkspace((current) => (current === null ? null : { ...current }));
      }
      requestAnimationFrame(() => commitErrorRef.current?.focus());
    } finally {
      setCommitStage("idle");
    }
  }

  async function applyGitCommit() {
    if (workspace === null || commitOperationBusy || commitPlan === null)
      return;
    setCommitError("");
    setCommitStage("applying");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<unknown>("git_commit_apply_open_workspace", {
        planToken: commitPlan.planToken,
      });
      if (!isGitCommitResultPayload(result))
        throw new Error("invalid Git commit result");
      if (!result.committed) {
        resetGitCommitUi();
        setAnnouncement("Git commit cancelled.");
        requestAnimationFrame(() => commitButtonRef.current?.focus());
        return;
      }
      resetGitCommitUi();
      if (result.workspaceChanged) await refreshWorkspace();
      else
        setWorkspace((current) => (current === null ? null : { ...current }));
      setAnnouncement(
        result.workspaceChanged
          ? "Git commit created. Newer Workspace changes remain uncommitted."
          : "Git commit created. No network connection was made.",
      );
      requestAnimationFrame(() => searchRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setCommitStage("idle");
      setCommitPlan(null);
      setCommitError(failure.message);
      setAnnouncement(failure.message);
      setWorkspace((current) => (current === null ? null : { ...current }));
      requestAnimationFrame(() => commitErrorRef.current?.focus());
    }
  }

  async function beginGitSync() {
    if (
      workspace === null ||
      gitStatus?.kind !== "ready" ||
      gitStatus.upstream === null ||
      syncPanelOpen ||
      syncOperationBusy ||
      commitFormOpen ||
      commitOperationBusy ||
      remoteFormOpen ||
      connectingRemote ||
      workspaceOperationBusy ||
      editingGuide ||
      editingGuideDetails ||
      creatingCategory ||
      renamingCategoryId !== "" ||
      creatingGuide ||
      dirtyGuide ||
      dirtyGuideDetails ||
      dirtyCategoryCreate ||
      dirtyCategoryRename ||
      dirtyGuideCreate
    )
      return;
    const generation = ++syncGenerationRef.current;
    setSyncPanelOpen(true);
    setSyncInspection(null);
    setSyncError("");
    setSyncStage("inspecting");
    requestAnimationFrame(() => syncPrimaryRef.current?.focus());
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<unknown>("git_sync_inspect_open_workspace");
      if (generation !== syncGenerationRef.current) return;
      if (!isGitSyncInspectionPayload(result)) {
        await invoke("git_sync_cancel_active").catch(() => undefined);
        throw new Error("invalid Git sync inspection result");
      }
      setSyncInspection(result);
      setSyncStage("idle");
      setAnnouncement(
        `${gitSyncPresentation(result).title}. Review the sync result.`,
      );
      requestAnimationFrame(() => syncPrimaryRef.current?.focus());
    } catch (error) {
      if (generation !== syncGenerationRef.current) return;
      const failure = nativeCommandError(error);
      setSyncStage("idle");
      if (failure.code === "cancelled") {
        resetGitSyncUi();
        setAnnouncement("Git sync inspection cancelled.");
        requestAnimationFrame(() => syncButtonRef.current?.focus());
        return;
      }
      setSyncError(failure.message);
      setAnnouncement(failure.message);
      if (failure.code === "recoveryRequired") setGitStatus(null);
      requestAnimationFrame(() => syncErrorRef.current?.focus());
    }
  }

  async function cancelGitSync() {
    if (
      !syncPanelOpen ||
      syncStage === "applying" ||
      syncStage === "recovering" ||
      syncStage === "cancelling"
    )
      return;
    const generation = ++syncGenerationRef.current;
    setSyncStage("cancelling");
    setSyncError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<unknown>("git_sync_cancel_active");
      if (!isGitSyncCancelResultPayload(result))
        throw new Error("invalid Git sync cancellation result");
      if (generation !== syncGenerationRef.current) return;
      resetGitSyncUi();
      setAnnouncement(
        result.cancellationRequested
          ? "Git sync cancelled."
          : "Git sync panel closed.",
      );
      requestAnimationFrame(() => syncButtonRef.current?.focus());
    } catch (error) {
      if (generation !== syncGenerationRef.current) return;
      const failure = nativeCommandError(error);
      setSyncStage("idle");
      setSyncError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => syncErrorRef.current?.focus());
    }
  }

  function installSyncedWorkspace(
    completed: WorkspacePayload,
    message: string,
  ) {
    setWorkspace(completed);
    setGitStatus(null);
    setExternalChangeWorkspaceId("");
    setWatchDegradedWorkspaceId(
      completed.watchStatus === "degraded" ? completed.id : "",
    );
    setSelectedId((current) =>
      completed.guides.some((guide) => guide.id === current)
        ? current
        : (completed.guides[0]?.id ?? ""),
    );
    resetGitSyncUi();
    setAnnouncement(message);
    requestAnimationFrame(() => searchRef.current?.focus());
  }

  async function applyGitSync() {
    if (
      workspace === null ||
      syncInspection === null ||
      syncOperationBusy ||
      !gitSyncPresentation(syncInspection).canApply
    )
      return;
    setSyncStage("applying");
    setSyncError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<unknown>("git_sync_apply_open_workspace", {
        planToken: syncInspection.planToken,
      });
      if (!isGitSyncApplyResultPayload(result))
        throw new Error("invalid Git sync apply result");
      if (result.kind === "declined") {
        resetGitSyncUi();
        setAnnouncement("Git fast-forward cancelled.");
        requestAnimationFrame(() => syncButtonRef.current?.focus());
        return;
      }
      installSyncedWorkspace(
        result.workspace,
        result.changed
          ? "Workspace fast-forward applied successfully."
          : "Git sync completed with no Workspace changes.",
      );
    } catch (error) {
      const failure = nativeCommandError(error);
      setSyncStage("idle");
      setSyncInspection(null);
      setSyncError(failure.message);
      setGitStatus(null);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => syncErrorRef.current?.focus());
    }
  }

  async function applyGitPush() {
    if (
      workspace === null ||
      syncInspection === null ||
      syncOperationBusy ||
      !gitSyncPresentation(syncInspection).canPush
    )
      return;
    setSyncStage("applying");
    setSyncError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const nativeInvoke: NativeInvoker = (command, arguments_) =>
        invoke(command, arguments_);
      const result = await requestGitPush(
        syncInspection.planToken,
        nativeInvoke,
      );
      if (result.kind === "declined") {
        resetGitSyncUi();
        setAnnouncement("Git push cancelled.");
        focusGitSyncAction(() => syncButtonRef.current);
        return;
      }
      resetGitSyncUi();
      setGitStatus(null);
      try {
        setGitStatus(await requestGitStatus(nativeInvoke));
        setAnnouncement("Reviewed commit pushed successfully.");
      } catch {
        setGitStatus({
          kind: "failed",
          version: null,
          branch: null,
          detached: false,
          upstream: null,
          divergenceKnown: false,
          ahead: 0,
          behind: 0,
          changed: 0,
          untracked: 0,
          conflicts: 0,
          remotes: [],
          remoteCount: 0,
          failure: "Git status unavailable",
        });
        setAnnouncement(
          "The commit was pushed, but Git status could not be refreshed.",
        );
      }
      focusGitSyncAction(() => syncButtonRef.current ?? searchRef.current);
    } catch (error) {
      const failure = nativeCommandError(error);
      setSyncStage("idle");
      setSyncInspection(null);
      setSyncError(failure.message);
      setGitStatus(null);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => syncErrorRef.current?.focus());
    }
  }

  async function recoverGitOperation() {
    if (
      workspace === null ||
      gitStatus?.kind !== "recoveryRequired" ||
      commitOperationBusy ||
      syncOperationBusy
    )
      return;
    setSyncStage("recovering");
    setGitInitError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      try {
        const syncResult = await invoke<unknown>(
          "git_sync_recover_open_workspace",
        );
        if (!isGitSyncRecoveryResultPayload(syncResult))
          throw new Error("invalid Git sync recovery result");
        if (syncResult.kind === "declined") {
          setAnnouncement("Git sync recovery cancelled.");
          return;
        }
        installSyncedWorkspace(
          syncResult.workspace,
          "Interrupted Git sync recovered successfully.",
        );
        return;
      } catch (error) {
        const failure = nativeCommandError(error);
        if (failure.code !== "recoveryNotRequired") throw error;
      }
      const commitResult = await invoke<unknown>(
        "git_commit_recover_open_workspace",
      );
      if (!isGitCommitRecoveryResultPayload(commitResult))
        throw new Error("invalid Git commit recovery result");
      if (!commitResult.recovered) {
        setAnnouncement("Git commit recovery cancelled.");
        return;
      }
      if (commitResult.workspaceChanged) await refreshWorkspace();
      else
        setWorkspace((current) => (current === null ? null : { ...current }));
      setAnnouncement(
        commitResult.workspaceChanged
          ? "Git commit recovered. Newer Workspace changes remain uncommitted."
          : "Git commit recovered successfully.",
      );
    } catch (error) {
      const failure = nativeCommandError(error);
      setGitInitError(failure.message);
      setAnnouncement(failure.message);
    } finally {
      setSyncStage("idle");
    }
  }

  async function cancelWorkspaceSetup() {
    if (initializingWorkspace || gitCloneOperationBusy) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("cancel_selected_workspace_initialization");
    } catch {
      setAnnouncement("The Workspace selection could not be cleared.");
      return;
    }
    setPendingWorkspace(null);
    setWorkspaceName("");
    setWorkspaceSetupError("");
    resetGitCloneUi();
    setAnnouncement("Workspace setup cancelled.");
    requestAnimationFrame(() => searchRef.current?.focus());
  }

  function beginCategoryCreation() {
    if (workspace === null || !workspace.facts.writable) return;
    setCategoryName("");
    setCategorySummary("");
    setCategoryError("");
    setCreatingCategory(true);
    requestAnimationFrame(() => categoryNameRef.current?.focus());
  }

  function cancelCategoryCreation() {
    if (savingCategory) return;
    setCreatingCategory(false);
    setCategoryName("");
    setCategorySummary("");
    setCategoryError("");
    requestAnimationFrame(() => categoryCreateButtonRef.current?.focus());
  }

  async function saveCategory() {
    const details = normalizedCategoryDetails(categoryName, categorySummary);
    if (details === null || savingCategory) return;
    setSavingCategory(true);
    setCategoryError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const created = await invoke<CreateCategoryPayload>(
        "create_open_category",
        details,
      );
      setWorkspace(created.workspace);
      setCreatingCategory(false);
      setCategoryName("");
      setCategorySummary("");
      setAnnouncement(`${details.name} Category created.`);
      requestAnimationFrame(() => categoryCreateButtonRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setCategoryError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => categoryErrorRef.current?.focus());
    } finally {
      setSavingCategory(false);
    }
  }

  function beginCategoryRename(
    category: WorkspacePayload["categories"][number],
  ) {
    if (
      workspace === null ||
      !workspace.facts.writable ||
      reorderingCategoryId !== "" ||
      removingCategoryId !== ""
    )
      return;
    setRenamingCategoryId(category.id);
    setCategoryRenameValue(category.name);
    setCategoryRenameSummary(category.summary ?? "");
    setCategoryRenameError("");
    requestAnimationFrame(() => categoryRenameInputRef.current?.focus());
  }

  function cancelCategoryRename() {
    if (savingCategoryRename) return;
    if (
      dirtyCategoryRename &&
      !window.confirm("Discard these Category detail changes?")
    )
      return;
    const categoryId = renamingCategoryId;
    setRenamingCategoryId("");
    setCategoryRenameValue("");
    setCategoryRenameSummary("");
    setCategoryRenameError("");
    requestAnimationFrame(() =>
      categoryRenameButtonRefs.current.get(categoryId)?.focus(),
    );
  }

  async function saveCategoryRename() {
    const category = categoryBeingRenamed;
    const details = normalizedCategoryDetails(
      categoryRenameValue,
      categoryRenameSummary,
    );
    if (category === null || savingCategoryRename) return;
    if (details === null) {
      const message =
        "Enter a Category name from 1 to 200 characters and an optional summary up to 500 characters.";
      setCategoryRenameError(message);
      setAnnouncement(message);
      requestAnimationFrame(() => categoryRenameInputRef.current?.focus());
      return;
    }
    setSavingCategoryRename(true);
    setCategoryRenameError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveCategoryPayload>(
        "save_open_category_details",
        {
          categoryId: category.id,
          expectedRevision: category.revision,
          ...details,
        },
      );
      setWorkspace(result.workspace);
      setRenamingCategoryId("");
      setCategoryRenameValue("");
      setCategoryRenameSummary("");
      setAnnouncement(
        result.changed
          ? "Category details saved."
          : "Category details are unchanged.",
      );
      requestAnimationFrame(() =>
        categoryRenameButtonRefs.current.get(category.id)?.focus(),
      );
    } catch (error) {
      const failure = nativeCommandError(error);
      setCategoryRenameError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => categoryRenameErrorRef.current?.focus());
    } finally {
      setSavingCategoryRename(false);
    }
  }

  async function changeCategoryArchived(
    category: WorkspacePayload["categories"][number],
  ) {
    if (
      updatingCategoryStatusId !== "" ||
      reorderingCategoryId !== "" ||
      removingCategoryId !== ""
    )
      return;
    const archived = category.status === "active";
    setUpdatingCategoryStatusId(category.id);
    setCategoryStatusError(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveCategoryPayload>(
        "set_open_category_archived",
        {
          categoryId: category.id,
          expectedRevision: category.revision,
          archived,
        },
      );
      setWorkspace(result.workspace);
      setAnnouncement(
        result.changed
          ? archived
            ? `${category.name} Category archived.`
            : `${category.name} Category restored.`
          : `The ${category.name} Category status is unchanged.`,
      );
    } catch (error) {
      const failure = nativeCommandError(error);
      setCategoryStatusError({ id: category.id, message: failure.message });
      setAnnouncement(failure.message);
      requestAnimationFrame(() => categoryStatusErrorRef.current?.focus());
    } finally {
      setUpdatingCategoryStatusId("");
    }
  }

  async function moveCategory(
    category: WorkspacePayload["categories"][number],
    direction: "up" | "down",
  ) {
    if (
      workspace === null ||
      reorderingCategoryId !== "" ||
      updatingCategoryStatusId !== "" ||
      removingCategoryId !== ""
    )
      return;
    const moved = movedCategoryOrder(
      workspace.categories,
      category.id,
      direction,
    );
    if (moved === null) return;
    setReorderingCategoryId(category.id);
    setCategoryReorderError(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveCategoryPayload>(
        "reorder_open_categories",
        {
          ordered: moved.map((item) => ({
            id: item.id,
            expectedRevision: item.revision,
          })),
        },
      );
      setWorkspace(result.workspace);
      setAnnouncement(
        result.changed
          ? `${category.name} Category moved ${direction}.`
          : "Category order is unchanged.",
      );
    } catch (error) {
      const failure = nativeCommandError(error);
      setCategoryReorderError({ id: category.id, message: failure.message });
      setAnnouncement(failure.message);
      requestAnimationFrame(() => categoryReorderErrorRef.current?.focus());
    } finally {
      setReorderingCategoryId("");
    }
  }

  async function removeEmptyCategory(
    category: WorkspacePayload["categories"][number],
  ) {
    if (
      workspace === null ||
      category.guideCount !== 0 ||
      removingCategoryId !== "" ||
      reorderingCategoryId !== "" ||
      updatingCategoryStatusId !== ""
    )
      return;
    if (
      !window.confirm(
        `Remove the “${category.name}” Category? Its descriptor will move to Rehow Trash. This cannot remove Guides.`,
      )
    )
      return;
    setRemovingCategoryId(category.id);
    setCategoryStatusError(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveCategoryPayload>("remove_open_category", {
        categoryId: category.id,
        expectedRevision: category.revision,
      });
      setWorkspace(result.workspace);
      setAnnouncement(`${category.name} Category moved to Rehow Trash.`);
      requestAnimationFrame(() => categoryCreateButtonRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setCategoryStatusError({ id: category.id, message: failure.message });
      setAnnouncement(failure.message);
      requestAnimationFrame(() => categoryStatusErrorRef.current?.focus());
    } finally {
      setRemovingCategoryId("");
    }
  }

  function beginGuideCreation() {
    if (workspace === null || !workspace.facts.writable) return;
    const category = workspace.categories.find(
      (candidate) => candidate.status === "active",
    );
    if (!category) {
      setAnnouncement("Create an active Category before creating a Guide.");
      requestAnimationFrame(() => categoryCreateButtonRef.current?.focus());
      return;
    }
    setGuideTitle("");
    setGuideSummary("");
    setGuideCategoryId(category.id);
    setGuideCreateError("");
    setCreatingGuide(true);
    requestAnimationFrame(() => guideTitleRef.current?.focus());
  }

  function cancelGuideCreation() {
    if (savingNewGuide) return;
    if (
      (guideTitle.trim() !== "" || guideSummary.trim() !== "") &&
      !window.confirm("Discard this unfinished Guide draft?")
    )
      return;
    setCreatingGuide(false);
    setGuideTitle("");
    setGuideSummary("");
    setGuideCreateError("");
    requestAnimationFrame(() => guideCreateButtonRef.current?.focus());
  }

  async function saveNewGuide() {
    const details = normalizedGuideDetails(
      guideCategoryId,
      guideTitle,
      guideSummary,
    );
    if (details === null || savingNewGuide) return;
    setSavingNewGuide(true);
    setGuideCreateError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const created = await invoke<CreateGuidePayload>(
        "create_open_guide",
        details,
      );
      setWorkspace(created.workspace);
      setGuideBody(created.guide);
      setDraftBody(created.guide.body);
      setSelectedId(created.guide.id);
      setCreatingGuide(false);
      setEditingGuide(true);
      setReaderOpen(true);
      setQuery("");
      setAnnouncement(`${details.title} Guide created as a draft.`);
      requestAnimationFrame(() => editorRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setGuideCreateError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => guideCreateErrorRef.current?.focus());
    } finally {
      setSavingNewGuide(false);
    }
  }

  function beginEditingGuideDetails() {
    if (
      selected === null ||
      guideBody === null ||
      guideBody.id !== selected.id ||
      editingGuide
    )
      return;
    setDetailTitle(selected.title);
    setDetailSummary(selected.summary);
    setDetailCategoryId(selected.categoryId);
    setDetailTags(selected.tags.join("\n"));
    setDetailAliases(selected.aliases.join("\n"));
    setGuideDetailsError("");
    setEditingGuideDetails(true);
    requestAnimationFrame(() => detailTitleRef.current?.focus());
  }

  function cancelEditingGuideDetails() {
    if (savingGuideDetails) return;
    if (
      dirtyGuideDetails &&
      !window.confirm("Discard your unsaved Guide detail changes?")
    )
      return;
    setEditingGuideDetails(false);
    setGuideDetailsError("");
    requestAnimationFrame(() => editDetailsButtonRef.current?.focus());
  }

  async function saveGuideDetails() {
    const currentGuide = guideBody;
    const titleLength = Array.from(detailTitle.trim()).length;
    const summaryLength = Array.from(detailSummary.trim()).length;
    const normalizedTags = normalizedGuideTerms(detailTags, 64);
    const normalizedAliases = normalizedGuideTerms(detailAliases, 200);
    const details = normalizedGuideMetadata(
      detailTitle,
      detailSummary,
      detailTags,
      detailAliases,
      detailCategoryId,
    );
    if (details === null) {
      const message =
        "Enter a valid title and summary, with no more than 64 unique Tags or aliases (one per line).";
      setGuideDetailsError(message);
      setAnnouncement(message);
      requestAnimationFrame(() => {
        if (titleLength < 1 || titleLength > 200)
          detailTitleRef.current?.focus();
        else if (summaryLength < 1 || summaryLength > 500)
          detailSummaryRef.current?.focus();
        else if (detailCategoryId === "") detailCategoryRef.current?.focus();
        else if (normalizedTags === null) detailTagsRef.current?.focus();
        else if (normalizedAliases === null) detailAliasesRef.current?.focus();
        else detailTitleRef.current?.focus();
      });
      return;
    }
    if (
      currentGuide === null ||
      currentGuide.id !== selected?.id ||
      savingGuideDetails
    )
      return;
    setSavingGuideDetails(true);
    setGuideDetailsError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveGuidePayload>("save_open_guide_details", {
        guideId: currentGuide.id,
        expectedRevision: currentGuide.revision,
        ...details,
      });
      setWorkspace(result.workspace);
      setGuideBody(result.guide);
      setEditingGuideDetails(false);
      setAnnouncement(
        result.changed
          ? "Guide details saved."
          : "No Guide detail changes to save.",
      );
      requestAnimationFrame(() => editDetailsButtonRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setGuideDetailsError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => guideDetailsErrorRef.current?.focus());
    } finally {
      setSavingGuideDetails(false);
    }
  }

  function beginEditing() {
    if (
      guideBody === null ||
      guideBody.id !== selected?.id ||
      editingGuideDetails
    )
      return;
    setDraftBody(guideBody.body);
    setSaveError("");
    setEditingGuide(true);
    requestAnimationFrame(() => editorRef.current?.focus());
  }

  function cancelEditing() {
    if (!confirmDiscard()) return;
    setDraftBody(guideBody?.body ?? "");
    setSaveError("");
    setEditingGuide(false);
    requestAnimationFrame(() => editButtonRef.current?.focus());
  }

  async function saveGuide() {
    const currentGuide = guideBody;
    if (
      currentGuide === null ||
      currentGuide.id !== selected?.id ||
      savingGuide
    )
      return;
    setSavingGuide(true);
    setSaveError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveGuidePayload>("save_open_guide_body", {
        guideId: currentGuide.id,
        expectedRevision: currentGuide.revision,
        body: draftBody,
      });
      setWorkspace(result.workspace);
      setGuideBody(result.guide);
      setDraftBody(result.guide.body);
      setEditingGuide(false);
      setAnnouncement(
        result.changed ? "Guide saved." : "No Guide changes to save.",
      );
      requestAnimationFrame(() => editButtonRef.current?.focus());
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : "The Guide could not be saved. Check the Workspace and try again.";
      setSaveError(message);
      setAnnouncement(message);
      requestAnimationFrame(() => saveErrorRef.current?.focus());
    } finally {
      setSavingGuide(false);
    }
  }

  async function changeGuideArchived() {
    if (
      selected === null ||
      guideBody?.id !== selected.id ||
      updatingGuideStatus ||
      verifyingGuide ||
      importingAttachment ||
      removingAttachmentId !== "" ||
      removingGuide ||
      editingGuide ||
      editingGuideDetails
    )
      return;
    const archived = selected.status !== "Archived";
    setUpdatingGuideStatus(true);
    setGuideStatusError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveGuidePayload>("set_open_guide_archived", {
        guideId: selected.id,
        expectedRevision: guideBody.revision,
        archived,
      });
      setWorkspace(result.workspace);
      setGuideBody(result.guide);
      setDraftBody(result.guide.body);
      setAnnouncement(
        result.changed
          ? archived
            ? `${selected.title} archived.`
            : `${selected.title} restored.`
          : `${selected.title} status is unchanged.`,
      );
      requestAnimationFrame(() => guideStatusButtonRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setGuideStatusError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => guideStatusErrorRef.current?.focus());
    } finally {
      setUpdatingGuideStatus(false);
    }
  }

  async function markGuideTested() {
    if (
      selected === null ||
      !canMarkGuideTested(selected.status) ||
      guideBody?.id !== selected.id ||
      verifyingGuide ||
      importingAttachment ||
      removingAttachmentId !== "" ||
      updatingGuideStatus ||
      removingGuide ||
      editingGuide ||
      editingGuideDetails
    )
      return;
    setVerifyingGuide(true);
    setGuideStatusError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveGuidePayload>(
        "approve_open_guide_tested",
        {
          guideId: selected.id,
          expectedRevision: guideBody.revision,
        },
      );
      setWorkspace(result.workspace);
      setGuideBody(result.guide);
      setDraftBody(result.guide.body);
      setAnnouncement(
        result.changed
          ? `${selected.title} marked tested.`
          : "Tested confirmation cancelled.",
      );
      requestAnimationFrame(() =>
        (result.changed
          ? guideStatusButtonRef.current
          : guideVerifyButtonRef.current
        )?.focus(),
      );
    } catch (error) {
      const failure = nativeCommandError(error);
      setGuideStatusError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => guideStatusErrorRef.current?.focus());
    } finally {
      setVerifyingGuide(false);
    }
  }

  async function importAttachment() {
    if (
      selected === null ||
      selected.status === "Archived" ||
      guideBody?.id !== selected.id ||
      importingAttachment ||
      removingAttachmentId !== "" ||
      verifyingGuide ||
      updatingGuideStatus ||
      removingGuide ||
      editingGuide ||
      editingGuideDetails
    )
      return;
    setImportingAttachment(true);
    setGuideStatusError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveGuidePayload>(
        "import_open_guide_attachment",
        {
          guideId: selected.id,
          expectedRevision: guideBody.revision,
        },
      );
      setWorkspace(result.workspace);
      setGuideBody(result.guide);
      setDraftBody(result.guide.body);
      setAnnouncement(
        result.changed ? "Attachment added." : "Attachment import cancelled.",
      );
      requestAnimationFrame(() => attachmentImportButtonRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setGuideStatusError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => guideStatusErrorRef.current?.focus());
    } finally {
      setImportingAttachment(false);
    }
  }

  async function removeAttachment(attachmentId: string) {
    if (
      selected === null ||
      selected.status === "Archived" ||
      guideBody?.id !== selected.id ||
      removingAttachmentId !== "" ||
      importingAttachment ||
      verifyingGuide ||
      updatingGuideStatus ||
      removingGuide ||
      editingGuide ||
      editingGuideDetails
    )
      return;
    setRemovingAttachmentId(attachmentId);
    setGuideStatusError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<SaveGuidePayload>(
        "remove_open_guide_attachment",
        {
          guideId: selected.id,
          attachmentId,
          expectedRevision: guideBody.revision,
        },
      );
      setWorkspace(result.workspace);
      setGuideBody(result.guide);
      setDraftBody(result.guide.body);
      setAnnouncement(
        result.changed
          ? "Attachment removed."
          : "Attachment removal cancelled.",
      );
      requestAnimationFrame(() =>
        (result.changed
          ? attachmentImportButtonRef.current
          : attachmentRemoveButtonRefs.current.get(attachmentId)
        )?.focus(),
      );
    } catch (error) {
      const failure = nativeCommandError(error);
      setGuideStatusError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => guideStatusErrorRef.current?.focus());
    } finally {
      setRemovingAttachmentId("");
    }
  }

  async function removeGuide() {
    if (
      selected === null ||
      guideBody?.id !== selected.id ||
      removingGuide ||
      updatingGuideStatus ||
      verifyingGuide ||
      importingAttachment ||
      removingAttachmentId !== "" ||
      editingGuide ||
      editingGuideDetails
    )
      return;
    const removedTitle = selected.title;
    setRemovingGuide(true);
    setGuideStatusError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<RemoveGuidePayload>("remove_open_guide", {
        guideId: selected.id,
        expectedRevision: guideBody.revision,
      });
      if (!result.removed) {
        setAnnouncement("Guide deletion cancelled.");
        requestAnimationFrame(() => guideRemoveButtonRef.current?.focus());
        return;
      }
      const nextGuideId = result.workspace.guides[0]?.id ?? "";
      setWorkspace(result.workspace);
      setGuideBody(null);
      setDraftBody("");
      setSelectedId(nextGuideId);
      setReaderOpen(false);
      setAnnouncement(`${removedTitle} moved to Rehow Trash.`);
      requestAnimationFrame(() => searchRef.current?.focus());
    } catch (error) {
      const failure = nativeCommandError(error);
      setGuideStatusError(failure.message);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => guideStatusErrorRef.current?.focus());
    } finally {
      setRemovingGuide(false);
    }
  }

  return (
    <div
      className={`app-shell ${readerOpen ? "reader-is-open" : ""} ${railOpen ? "rail-is-open" : ""}`}
    >
      <a className="skip-link" href="#guide-content">
        Skip to Guide
      </a>
      <header className="app-toolbar">
        <div className="brand" translate="no">
          <span className="brand-mark">
            <Icon name="book" />
          </span>
          <strong>{APPLICATION_NAME}</strong>
        </div>
        <button
          ref={railToggleRef}
          className="icon-button rail-toggle"
          type="button"
          aria-label="Toggle Workspace navigation"
          aria-controls="workspace-navigation"
          aria-expanded={railOpen}
          onClick={() => setRailOpen((value) => !value)}
          title="Workspace navigation"
        >
          <Icon name="folder" />
        </button>
        <button
          className="workspace-switcher"
          type="button"
          onClick={openWorkspace}
          disabled={
            openingWorkspace ||
            gitCloneOperationBusy ||
            gitCloneRequiresResolution ||
            syncOperationBusy
          }
        >
          <span className="workspace-dot" aria-hidden="true" />
          {gitCloneOperationBusy
            ? "Clone in progress…"
            : gitCloneRequiresResolution
              ? gitCloneStage === "recovery"
                ? "Clone needs attention"
                : "Clone ready"
              : openingWorkspace
                ? "Opening…"
                : (workspace?.name ?? "Open Workspace")}
          <Icon name="chevron" />
        </button>
        {workspace !== null && externalChangePending && autoRefreshBlocked ? (
          <span className="workspace-health" role="status">
            Files changed on disk
          </span>
        ) : workspace !== null && watchDegraded ? (
          <span
            className="workspace-health workspace-health-warning"
            role="status"
          >
            Auto-refresh unavailable
          </span>
        ) : null}
        <div className="toolbar-actions">
          <button
            ref={workspaceRefreshRef}
            className="icon-button"
            type="button"
            aria-label={
              refreshingWorkspace ? "Refreshing Workspace" : "Refresh Workspace"
            }
            aria-busy={refreshingWorkspace}
            title={
              workspace === null
                ? "Open a Workspace to refresh"
                : refreshingWorkspace
                  ? "Refreshing Workspace…"
                  : externalChangePending && autoRefreshBlocked
                    ? "Files changed on disk; refresh when ready"
                    : watchDegraded
                      ? "Auto-refresh unavailable; refresh Workspace manually"
                      : "Refresh Workspace"
            }
            disabled={
              workspace === null ||
              workspaceOperationBusy ||
              openingWorkspace ||
              syncOperationBusy
            }
            onClick={() => void refreshWorkspace()}
          >
            <Icon name="refresh" />
          </button>
          <button
            ref={guideCreateButtonRef}
            className="button button-primary"
            type="button"
            onClick={beginGuideCreation}
            aria-expanded={creatingGuide}
            title={
              workspace === null
                ? "Open a Workspace to create a Guide"
                : !workspace.facts.writable
                  ? "This Workspace is read-only"
                  : !workspace.categories.some(
                        (category) => category.status === "active",
                      )
                    ? "Create an active Category first"
                    : "Create Guide"
            }
            disabled={
              workspace === null ||
              !workspace.facts.writable ||
              creatingGuide ||
              !workspace.categories.some(
                (category) => category.status === "active",
              )
            }
          >
            <Icon name="plus" />
            <span className="toolbar-action-label">New Guide</span>
          </button>
          <button
            className="icon-button"
            type="button"
            aria-label="Open settings"
            title="Settings"
          >
            <Icon name="settings" />
          </button>
        </div>
      </header>

      <aside
        id="workspace-navigation"
        className="workspace-rail"
        aria-label="Workspace Navigation"
      >
        <nav>
          <p className="rail-label">Library</p>
          <RailItem
            icon="book"
            label="All Guides"
            count={guides.length}
            active
          />
          <RailItem
            icon="folder"
            label="Categories"
            count={workspace?.categories.length ?? 0}
          />
          <RailItem icon="tag" label="Tags" count={0} />
          <RailItem
            icon="archive"
            label="Archive"
            count={guides.filter((guide) => guide.status === "Archived").length}
          />
        </nav>
        <div className="category-list">
          <div className="category-list-heading">
            <p className="rail-label">Categories</p>
            {workspace?.facts.writable && (
              <button
                ref={categoryCreateButtonRef}
                className="category-create-button"
                type="button"
                aria-label="Create Category"
                title="Create Category"
                aria-expanded={creatingCategory}
                onClick={beginCategoryCreation}
                disabled={creatingCategory}
              >
                <Icon name="plus" />
              </button>
            )}
          </div>
          {creatingCategory && (
            <form
              className="category-create-form"
              aria-busy={savingCategory}
              onSubmit={(event) => {
                event.preventDefault();
                void saveCategory();
              }}
            >
              <label htmlFor="category-name">Category Name</label>
              <input
                ref={categoryNameRef}
                id="category-name"
                name="category-name"
                type="text"
                autoComplete="off"
                maxLength={200}
                aria-invalid={categoryError !== ""}
                aria-describedby={
                  categoryError !== "" ? "category-create-error" : undefined
                }
                value={categoryName}
                onChange={(event) => {
                  noteDraftChange();
                  setCategoryName(event.target.value);
                  setCategoryError("");
                }}
              />
              <label htmlFor="category-summary">Summary (Optional)</label>
              <textarea
                id="category-summary"
                name="category-summary"
                autoComplete="off"
                maxLength={500}
                rows={3}
                aria-invalid={categoryError !== ""}
                aria-describedby={
                  categoryError !== "" ? "category-create-error" : undefined
                }
                value={categorySummary}
                onChange={(event) => {
                  noteDraftChange();
                  setCategorySummary(event.target.value);
                  setCategoryError("");
                }}
              />
              {categoryError !== "" && (
                <p
                  ref={categoryErrorRef}
                  id="category-create-error"
                  className="category-create-error"
                  role="alert"
                  tabIndex={-1}
                >
                  {categoryError}
                </p>
              )}
              <div className="category-create-actions">
                <button
                  className="button"
                  type="button"
                  disabled={savingCategory}
                  onClick={cancelCategoryCreation}
                >
                  Cancel
                </button>
                <button
                  className="button button-primary"
                  type="submit"
                  disabled={
                    savingCategory ||
                    normalizedCategoryDetails(categoryName, categorySummary) ===
                      null
                  }
                >
                  {savingCategory ? "Creating…" : "Create Category"}
                </button>
              </div>
            </form>
          )}
          {(workspace?.categories ?? []).map((category, categoryIndex) =>
            renamingCategoryId === category.id ? (
              <form
                className="category-rename-form"
                key={category.id}
                aria-busy={savingCategoryRename}
                onSubmit={(event) => {
                  event.preventDefault();
                  void saveCategoryRename();
                }}
              >
                <label htmlFor={`category-rename-${category.id}`}>
                  Category Name
                </label>
                <input
                  ref={categoryRenameInputRef}
                  id={`category-rename-${category.id}`}
                  name="category-rename"
                  type="text"
                  autoComplete="off"
                  maxLength={200}
                  aria-invalid={categoryRenameError !== ""}
                  aria-describedby={
                    categoryRenameError !== ""
                      ? "category-rename-error"
                      : undefined
                  }
                  value={categoryRenameValue}
                  onChange={(event) => {
                    noteDraftChange();
                    setCategoryRenameValue(event.target.value);
                    setCategoryRenameError("");
                  }}
                />
                <label htmlFor={`category-summary-${category.id}`}>
                  Summary (Optional)
                </label>
                <textarea
                  id={`category-summary-${category.id}`}
                  name="category-summary"
                  autoComplete="off"
                  maxLength={500}
                  rows={3}
                  aria-invalid={categoryRenameError !== ""}
                  aria-describedby={
                    categoryRenameError !== ""
                      ? "category-rename-error"
                      : undefined
                  }
                  value={categoryRenameSummary}
                  onChange={(event) => {
                    noteDraftChange();
                    setCategoryRenameSummary(event.target.value);
                    setCategoryRenameError("");
                  }}
                />
                {categoryRenameError !== "" && (
                  <p
                    ref={categoryRenameErrorRef}
                    id="category-rename-error"
                    className="category-create-error"
                    role="alert"
                    tabIndex={-1}
                  >
                    {categoryRenameError}
                  </p>
                )}
                <div className="category-create-actions">
                  <button
                    className="button"
                    type="button"
                    disabled={savingCategoryRename}
                    onClick={cancelCategoryRename}
                  >
                    Cancel
                  </button>
                  <button
                    className="button button-primary"
                    type="submit"
                    disabled={savingCategoryRename}
                  >
                    {savingCategoryRename ? "Saving…" : "Save Details"}
                  </button>
                </div>
              </form>
            ) : (
              <div
                className="category-entry"
                key={category.id}
                aria-busy={
                  reorderingCategoryId === category.id ||
                  removingCategoryId === category.id
                }
              >
                <div className="category-item">
                  <span>{category.name}</span>
                  {category.status === "archived" && (
                    <span className="category-state">Archived</span>
                  )}
                  {reorderingCategoryId === category.id && (
                    <span className="category-state">Moving…</span>
                  )}
                  {removingCategoryId === category.id && (
                    <span className="category-state">Removing…</span>
                  )}
                  <span className="count">{category.guideCount}</span>
                  {workspace?.facts.writable && (
                    <span className="category-item-actions">
                      <button
                        ref={(element) => {
                          if (element === null) {
                            categoryRenameButtonRefs.current.delete(
                              category.id,
                            );
                          } else {
                            categoryRenameButtonRefs.current.set(
                              category.id,
                              element,
                            );
                          }
                        }}
                        className="category-rename-button"
                        type="button"
                        aria-label={`Edit ${category.name} Category details`}
                        title="Edit Category details"
                        disabled={
                          updatingCategoryStatusId !== "" ||
                          reorderingCategoryId !== "" ||
                          removingCategoryId !== ""
                        }
                        onClick={() => beginCategoryRename(category)}
                      >
                        Edit
                      </button>
                      <button
                        className="category-rename-button category-order-button"
                        type="button"
                        aria-label={`Move ${category.name} Category up`}
                        title="Move Category up"
                        disabled={
                          categoryIndex === 0 ||
                          updatingCategoryStatusId !== "" ||
                          reorderingCategoryId !== "" ||
                          removingCategoryId !== ""
                        }
                        onClick={() => void moveCategory(category, "up")}
                      >
                        <span aria-hidden="true">↑</span>
                      </button>
                      <button
                        className="category-rename-button category-order-button"
                        type="button"
                        aria-label={`Move ${category.name} Category down`}
                        title="Move Category down"
                        disabled={
                          categoryIndex ===
                            (workspace?.categories.length ?? 0) - 1 ||
                          updatingCategoryStatusId !== "" ||
                          reorderingCategoryId !== "" ||
                          removingCategoryId !== ""
                        }
                        onClick={() => void moveCategory(category, "down")}
                      >
                        <span aria-hidden="true">↓</span>
                      </button>
                      <button
                        className="category-rename-button"
                        type="button"
                        disabled={
                          updatingCategoryStatusId !== "" ||
                          reorderingCategoryId !== "" ||
                          removingCategoryId !== ""
                        }
                        onClick={() => void changeCategoryArchived(category)}
                      >
                        {updatingCategoryStatusId === category.id
                          ? category.status === "active"
                            ? "Archiving…"
                            : "Restoring…"
                          : category.status === "active"
                            ? "Archive"
                            : "Restore"}
                      </button>
                      {category.guideCount === 0 && (
                        <button
                          className="category-rename-button category-remove-button"
                          type="button"
                          disabled={
                            updatingCategoryStatusId !== "" ||
                            reorderingCategoryId !== "" ||
                            removingCategoryId !== ""
                          }
                          onClick={() => void removeEmptyCategory(category)}
                        >
                          {removingCategoryId === category.id
                            ? "Removing…"
                            : "Remove"}
                        </button>
                      )}
                    </span>
                  )}
                </div>
                {categoryStatusError?.id === category.id && (
                  <p
                    ref={categoryStatusErrorRef}
                    className="category-status-error"
                    role="alert"
                    tabIndex={-1}
                  >
                    {categoryStatusError.message}
                  </p>
                )}
                {categoryReorderError?.id === category.id && (
                  <p
                    ref={categoryReorderErrorRef}
                    className="category-status-error"
                    role="alert"
                    tabIndex={-1}
                  >
                    {categoryReorderError.message}
                  </p>
                )}
              </div>
            ),
          )}
        </div>
        <div className="rail-footer">
          <span className="sync-indicator" aria-hidden="true" />
          {workspace === null
            ? "No Workspace open"
            : workspace.issueCount === 0
              ? workspace.facts.writable
                ? "Workspace ready"
                : "Workspace open read-only"
              : `${workspace.issueCount} item${workspace.issueCount === 1 ? "" : "s"} need attention`}
        </div>
      </aside>

      <section className="results-pane" aria-label="Guide Search & Results">
        <div className="search-area">
          <label className="search-box" htmlFor="guide-search">
            <Icon name="search" />
            <span className="sr-only">Search Guides</span>
            <input
              ref={searchRef}
              id="guide-search"
              name="guide-search"
              type="search"
              autoComplete="off"
              maxLength={512}
              placeholder="Search Guides…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <span aria-hidden="true">
              <kbd>Ctrl K</kbd>
            </span>
          </label>
          <div className="results-meta">
            <span aria-live="polite" aria-atomic="true">
              {resultCountLabel(visibleGuides.length)}
            </span>
            <label className="search-scope" htmlFor="search-scope">
              <span className="sr-only">Guide search scope</span>
              <select
                id="search-scope"
                value={searchScope}
                onChange={(event) =>
                  changeSearchScope(event.target.value as SearchScope)
                }
              >
                <option value="active">Active Guides</option>
                <option value="tested">Tested</option>
                <option value="draft">Draft</option>
                <option value="stale">Stale</option>
                <option value="archived">Archived</option>
                <option value="all">All Guides</option>
              </select>
            </label>
          </div>
          {announcement !== "" && (
            <p className="workspace-message" role="status">
              {announcement}
            </p>
          )}
        </div>
        {creatingGuide && workspace !== null && (
          <form
            className="guide-create-form"
            aria-busy={savingNewGuide}
            onSubmit={(event) => {
              event.preventDefault();
              void saveNewGuide();
            }}
          >
            <div className="guide-create-heading">
              <div>
                <h2>Create Guide</h2>
                <p>Start a draft, then write the steps in the editor.</p>
              </div>
              <button
                className="button"
                type="button"
                disabled={savingNewGuide}
                onClick={cancelGuideCreation}
              >
                Cancel
              </button>
            </div>
            <div className="guide-create-fields">
              <label htmlFor="new-guide-title">Title</label>
              <input
                ref={guideTitleRef}
                id="new-guide-title"
                name="new-guide-title"
                type="text"
                autoComplete="off"
                maxLength={200}
                aria-invalid={guideCreateError !== ""}
                aria-describedby={
                  guideCreateError !== "" ? "guide-create-error" : undefined
                }
                value={guideTitle}
                onChange={(event) => {
                  noteDraftChange();
                  setGuideTitle(event.target.value);
                  setGuideCreateError("");
                }}
              />
              <label htmlFor="new-guide-summary">Summary</label>
              <textarea
                id="new-guide-summary"
                name="new-guide-summary"
                autoComplete="off"
                maxLength={500}
                rows={2}
                aria-invalid={guideCreateError !== ""}
                aria-describedby={
                  guideCreateError !== "" ? "guide-create-error" : undefined
                }
                value={guideSummary}
                onChange={(event) => {
                  noteDraftChange();
                  setGuideSummary(event.target.value);
                  setGuideCreateError("");
                }}
              />
              <label htmlFor="new-guide-category">Category</label>
              <select
                id="new-guide-category"
                name="new-guide-category"
                autoComplete="off"
                aria-invalid={guideCreateError !== ""}
                aria-describedby={
                  guideCreateError !== "" ? "guide-create-error" : undefined
                }
                value={guideCategoryId}
                onChange={(event) => {
                  noteDraftChange();
                  setGuideCategoryId(event.target.value);
                  setGuideCreateError("");
                }}
              >
                {workspace.categories
                  .filter((category) => category.status === "active")
                  .map((category) => (
                    <option key={category.id} value={category.id}>
                      {category.name}
                    </option>
                  ))}
              </select>
            </div>
            {guideCreateError !== "" && (
              <p
                ref={guideCreateErrorRef}
                id="guide-create-error"
                className="guide-create-error"
                role="alert"
                tabIndex={-1}
              >
                {guideCreateError}
              </p>
            )}
            <div className="guide-create-actions">
              <button
                className="button button-primary"
                type="submit"
                disabled={
                  savingNewGuide ||
                  normalizedGuideDetails(
                    guideCategoryId,
                    guideTitle,
                    guideSummary,
                  ) === null
                }
              >
                {savingNewGuide ? "Creating…" : "Create & Edit Guide"}
              </button>
            </div>
          </form>
        )}
        <ul ref={resultListRef} className="result-list" aria-label="Guides">
          {visibleGuides.length > 0 ? (
            visibleGuides.map((guide, index) => {
              const searchHit = searchHitsByGuideId.get(guide.id);
              return (
                <li key={guide.id}>
                  <button
                    type="button"
                    className={`result-row ${guide.id === selected?.id ? "is-selected" : ""}`}
                    aria-current={
                      guide.id === selected?.id ? "true" : undefined
                    }
                    onClick={(event) =>
                      openGuide(guide.id, event.currentTarget)
                    }
                    onKeyDown={(event) => handleResultKey(event, index)}
                  >
                    <span className="result-heading">
                      <strong>{guide.title}</strong>
                      <span>{formatUpdated(guide.updatedAt)}</span>
                    </span>
                    {query.trim() !== "" && searchHit !== undefined ? (
                      <SearchExcerpt hit={searchHit} />
                    ) : (
                      <span className="result-summary">{guide.summary}</span>
                    )}
                    <span className="result-footer">
                      <span>{guide.category}</span>
                      <Status value={guide.status} />
                    </span>
                  </button>
                </li>
              );
            })
          ) : (
            <li className="empty-results">
              <Icon name={pendingWorkspace === null ? "search" : "folder"} />
              <h2>
                {pendingWorkspace !== null
                  ? gitCloneStage === "idle"
                    ? "Create This Workspace"
                    : "Clone a Git Workspace"
                  : workspace === null
                    ? "Open a Workspace"
                    : guides.length === 0 && query.trim() === ""
                      ? "No Guides Yet"
                      : query.trim() === ""
                        ? "No Guides in This Scope"
                        : "No Matching Guides"}
              </h2>
              <p>
                {pendingWorkspace !== null
                  ? pendingWorkspace.writable
                    ? gitCloneStage === "idle"
                      ? "The selected folder is empty. Create a new Workspace or clone an existing one."
                      : gitCloneStage === "entry"
                        ? "Enter an HTTPS or SSH repository address. Rehow validates it privately before changing the selected folder."
                        : gitCloneStage === "validating" ||
                            gitCloneStage === "cancelling"
                          ? "Rehow is validating the remote Workspace in private staging. The selected folder remains unchanged."
                          : gitCloneStage === "recovery"
                            ? "Native clone state needs reconciliation before Rehow can safely continue."
                            : "Validation passed. Complete the clone to write the exact validated Workspace and Git metadata."
                    : "The selected folder is read-only. Choose a writable empty folder."
                  : workspace === null
                    ? "Choose a Rehow Workspace to load its Guides securely."
                    : guides.length === 0 && query.trim() === ""
                      ? "This Workspace is ready for its first Guide."
                      : query.trim() === ""
                        ? "Choose another status scope to see more Guides."
                        : "Try fewer words, another scope, or search for a command, path, or error message."}
              </p>
              {workspaceSetupError !== "" && (
                <p
                  ref={workspaceSetupErrorRef}
                  id="workspace-setup-error"
                  className="workspace-setup-error"
                  role="alert"
                  tabIndex={-1}
                >
                  {workspaceSetupError}
                </p>
              )}
              {pendingWorkspace?.writable ? (
                gitCloneStage === "idle" ? (
                  <div className="workspace-onboarding-options">
                    <form
                      className="workspace-setup"
                      aria-busy={initializingWorkspace}
                      onSubmit={(event) => {
                        event.preventDefault();
                        void createWorkspace();
                      }}
                    >
                      <label htmlFor="workspace-name">Workspace Name</label>
                      <input
                        ref={workspaceNameRef}
                        id="workspace-name"
                        name="workspace-name"
                        type="text"
                        autoComplete="off"
                        maxLength={200}
                        aria-invalid={workspaceSetupError !== ""}
                        aria-describedby={
                          workspaceSetupError !== ""
                            ? "workspace-setup-error"
                            : undefined
                        }
                        value={workspaceName}
                        onChange={(event) => {
                          setWorkspaceName(event.target.value);
                          setWorkspaceSetupError("");
                        }}
                      />
                      <div className="workspace-setup-actions">
                        <button
                          className="button"
                          type="button"
                          disabled={initializingWorkspace}
                          onClick={() => void cancelWorkspaceSetup()}
                        >
                          Cancel
                        </button>
                        <button
                          className="button button-primary"
                          type="submit"
                          disabled={
                            initializingWorkspace ||
                            normalizedWorkspaceName(workspaceName) === null
                          }
                        >
                          {initializingWorkspace
                            ? "Creating…"
                            : "Create Workspace"}
                        </button>
                      </div>
                    </form>
                    <div className="workspace-onboarding-alternative">
                      <span>Already use Rehow with Git?</span>
                      <button
                        className="button"
                        type="button"
                        disabled={initializingWorkspace}
                        onClick={beginGitClone}
                      >
                        Clone Git Workspace
                      </button>
                    </div>
                  </div>
                ) : gitCloneStage === "entry" ? (
                  <form
                    className="workspace-setup"
                    onSubmit={(event) => void validateGitClone(event)}
                  >
                    <label htmlFor="git-clone-url">
                      Git Repository Address
                    </label>
                    <input
                      ref={gitCloneUrlRef}
                      id="git-clone-url"
                      name="git-clone-url"
                      type="text"
                      inputMode="url"
                      autoComplete="off"
                      spellCheck={false}
                      maxLength={2048}
                      placeholder="https://host/you/rehow.git or git@host:you/rehow.git"
                      aria-invalid={workspaceSetupError !== ""}
                      aria-describedby={
                        workspaceSetupError !== ""
                          ? "workspace-setup-error"
                          : "git-clone-help"
                      }
                      value={gitCloneUrl}
                      onChange={(event) => {
                        setGitCloneUrl(event.target.value);
                        setWorkspaceSetupError("");
                      }}
                    />
                    <span id="git-clone-help" className="workspace-setup-help">
                      Use canonical HTTPS, ssh://, or user@host:path. Do not
                      paste passwords, private keys, or access tokens.
                    </span>
                    <div className="workspace-setup-actions">
                      <button
                        className="button"
                        type="button"
                        onClick={returnToWorkspaceCreation}
                      >
                        Back
                      </button>
                      <button className="button button-primary" type="submit">
                        Validate Workspace
                      </button>
                    </div>
                  </form>
                ) : gitCloneStage === "validating" ||
                  gitCloneStage === "cancelling" ? (
                  <div
                    className="workspace-clone-progress"
                    role="status"
                    aria-live="polite"
                    aria-busy="true"
                  >
                    <progress aria-label="Git Workspace validation in progress" />
                    <strong>
                      {gitCloneStage === "cancelling"
                        ? "Cancelling Validation…"
                        : "Validating Workspace…"}
                    </strong>
                    <span>
                      {gitCloneHost === ""
                        ? "Preparing the native validation request."
                        : `Checking the bounded Workspace from ${gitCloneHost}.`}
                    </span>
                    <button
                      className="button"
                      type="button"
                      disabled={gitCloneStage === "cancelling"}
                      onClick={() => void cancelGitCloneValidation()}
                    >
                      {gitCloneStage === "cancelling"
                        ? "Cancelling…"
                        : "Cancel Validation"}
                    </button>
                  </div>
                ) : (
                  <div className="workspace-clone-ready" aria-live="polite">
                    <span className="workspace-clone-kicker">
                      {gitCloneStage === "recovery"
                        ? "State Reconciliation"
                        : "Validation Complete"}
                    </span>
                    <strong>
                      {gitCloneStage === "recovery"
                        ? "Clone Needs Attention"
                        : gitCloneHost === ""
                          ? "Git Workspace Ready"
                          : `${gitCloneHost} is ready`}
                    </strong>
                    <span>
                      {gitCloneStage === "recovery"
                        ? "Retry completion to reconcile native state, or cancel this Workspace setup."
                        : "The selected folder has not changed. Completion uses a separate native confirmation."}
                    </span>
                    <div className="workspace-setup-actions">
                      <button
                        className="button"
                        type="button"
                        disabled={gitCloneStage === "completing"}
                        onClick={() => void cancelWorkspaceSetup()}
                      >
                        Cancel Clone
                      </button>
                      <button
                        ref={gitCloneCompleteRef}
                        className="button button-primary"
                        type="button"
                        disabled={gitCloneStage === "completing"}
                        onClick={() => void completeGitClone()}
                      >
                        {gitCloneStage === "completing"
                          ? "Completing Clone…"
                          : gitCloneStage === "recovery"
                            ? "Reconcile Clone"
                            : "Complete Clone"}
                      </button>
                    </div>
                  </div>
                )
              ) : workspace === null ? (
                <button
                  className="button"
                  type="button"
                  onClick={openWorkspace}
                >
                  Open Workspace
                </button>
              ) : guides.length === 0 && query.trim() === "" ? null : (
                <div className="empty-results-actions">
                  {query.trim() !== "" && (
                    <button
                      className="button"
                      type="button"
                      onClick={() => setQuery("")}
                    >
                      Clear Search
                    </button>
                  )}
                  {searchScope !== "active" && (
                    <button
                      className="button"
                      type="button"
                      onClick={() => changeSearchScope("active")}
                    >
                      Show Active Guides
                    </button>
                  )}
                  {query.trim() !== "" &&
                    searchScope !== "archived" &&
                    searchScope !== "all" && (
                      <button
                        className="button"
                        type="button"
                        onClick={() => changeSearchScope("archived")}
                      >
                        Search Archived
                      </button>
                    )}
                </div>
              )}
            </li>
          )}
        </ul>
      </section>

      <main
        ref={readerRef}
        className="reader-pane"
        id="guide-content"
        tabIndex={-1}
      >
        {selected === null ? (
          <article className="guide-reader reader-placeholder">
            <Icon name="book" />
            <h1>No Guide Selected</h1>
            <p>Open a Workspace, then choose a Guide from the results.</p>
          </article>
        ) : (
          <article className="guide-reader">
            <button className="back-button" type="button" onClick={closeReader}>
              ← Back to Results
            </button>
            <header className="guide-header">
              <div className="breadcrumbs">
                <span>{selected.category}</span>
              </div>
              <div className="title-line">
                <h1>{selected.title}</h1>
                <Status value={selected.status} />
                {workspace?.facts.writable &&
                  !editingGuide &&
                  !editingGuideDetails && (
                    <div className="guide-header-actions">
                      <button
                        ref={editDetailsButtonRef}
                        className="button guide-edit-button"
                        type="button"
                        onClick={beginEditingGuideDetails}
                        disabled={
                          guideBody?.id !== selected.id ||
                          updatingGuideStatus ||
                          verifyingGuide ||
                          importingAttachment ||
                          removingAttachmentId !== "" ||
                          removingGuide
                        }
                      >
                        Edit Details
                      </button>
                      <button
                        ref={editButtonRef}
                        className="button guide-edit-button"
                        type="button"
                        onClick={beginEditing}
                        disabled={
                          guideBody?.id !== selected.id ||
                          updatingGuideStatus ||
                          verifyingGuide ||
                          importingAttachment ||
                          removingAttachmentId !== "" ||
                          removingGuide
                        }
                      >
                        Edit Content
                      </button>
                      {canMarkGuideTested(selected.status) && (
                        <button
                          ref={guideVerifyButtonRef}
                          className="button guide-verify-button"
                          type="button"
                          onClick={() => void markGuideTested()}
                          disabled={
                            guideBody?.id !== selected.id ||
                            verifyingGuide ||
                            importingAttachment ||
                            removingAttachmentId !== "" ||
                            updatingGuideStatus ||
                            removingGuide
                          }
                        >
                          {verifyingGuide ? "Confirming…" : "Mark Tested"}
                        </button>
                      )}
                      {selected.status !== "Archived" && (
                        <button
                          ref={attachmentImportButtonRef}
                          className="button guide-edit-button"
                          type="button"
                          onClick={() => void importAttachment()}
                          disabled={
                            guideBody?.id !== selected.id ||
                            importingAttachment ||
                            removingAttachmentId !== "" ||
                            verifyingGuide ||
                            updatingGuideStatus ||
                            removingGuide
                          }
                        >
                          {importingAttachment
                            ? "Importing…"
                            : "Add Attachment"}
                        </button>
                      )}
                      <button
                        ref={guideStatusButtonRef}
                        className="button guide-status-button"
                        type="button"
                        onClick={() => void changeGuideArchived()}
                        disabled={
                          guideBody?.id !== selected.id ||
                          updatingGuideStatus ||
                          verifyingGuide ||
                          importingAttachment ||
                          removingAttachmentId !== "" ||
                          removingGuide
                        }
                      >
                        {guideLifecycleLabel(
                          selected.status,
                          updatingGuideStatus,
                        )}
                      </button>
                      <button
                        ref={guideRemoveButtonRef}
                        className="button guide-remove-button"
                        type="button"
                        onClick={() => void removeGuide()}
                        disabled={
                          guideBody?.id !== selected.id ||
                          updatingGuideStatus ||
                          verifyingGuide ||
                          importingAttachment ||
                          removingAttachmentId !== "" ||
                          removingGuide
                        }
                      >
                        {removingGuide ? "Deleting…" : "Delete Guide"}
                      </button>
                    </div>
                  )}
              </div>
              {guideStatusError !== "" && (
                <p
                  ref={guideStatusErrorRef}
                  className="editor-error guide-status-error"
                  role="alert"
                  tabIndex={-1}
                >
                  {guideStatusError}
                </p>
              )}
              {editingGuideDetails && guideBody?.id === selected.id ? (
                <form
                  className="guide-details-form"
                  aria-busy={savingGuideDetails}
                  onSubmit={(event) => {
                    event.preventDefault();
                    void saveGuideDetails();
                  }}
                >
                  <label htmlFor="guide-detail-title">Title</label>
                  <input
                    ref={detailTitleRef}
                    id="guide-detail-title"
                    name="guide-detail-title"
                    type="text"
                    autoComplete="off"
                    maxLength={200}
                    aria-invalid={guideDetailsError !== ""}
                    aria-describedby={
                      guideDetailsError !== ""
                        ? "guide-details-error"
                        : undefined
                    }
                    value={detailTitle}
                    onChange={(event) => {
                      noteDraftChange();
                      setDetailTitle(event.target.value);
                      setGuideDetailsError("");
                    }}
                  />
                  <label htmlFor="guide-detail-summary">Summary</label>
                  <textarea
                    ref={detailSummaryRef}
                    id="guide-detail-summary"
                    name="guide-detail-summary"
                    autoComplete="off"
                    maxLength={500}
                    rows={3}
                    aria-invalid={guideDetailsError !== ""}
                    aria-describedby={
                      guideDetailsError !== ""
                        ? "guide-details-error"
                        : undefined
                    }
                    value={detailSummary}
                    onChange={(event) => {
                      noteDraftChange();
                      setDetailSummary(event.target.value);
                      setGuideDetailsError("");
                    }}
                  />
                  <label htmlFor="guide-detail-category">Category</label>
                  <select
                    ref={detailCategoryRef}
                    id="guide-detail-category"
                    name="guide-detail-category"
                    autoComplete="off"
                    aria-invalid={guideDetailsError !== ""}
                    aria-describedby={
                      guideDetailsError !== ""
                        ? "guide-detail-category-help guide-details-error"
                        : "guide-detail-category-help"
                    }
                    value={detailCategoryId}
                    onChange={(event) => {
                      noteDraftChange();
                      setDetailCategoryId(event.target.value);
                      setGuideDetailsError("");
                    }}
                  >
                    {(workspace?.categories ?? [])
                      .filter(
                        (category) =>
                          category.status === "active" ||
                          category.id === selected.categoryId,
                      )
                      .map((category) => (
                        <option key={category.id} value={category.id}>
                          {category.name}
                          {category.status === "archived" ? " (Archived)" : ""}
                        </option>
                      ))}
                  </select>
                  <p className="field-help" id="guide-detail-category-help">
                    Moving a Guide preserves its filename and Attachments.
                  </p>
                  <label htmlFor="guide-detail-tags">Tags</label>
                  <p className="field-help" id="guide-detail-tags-help">
                    One Tag per line, up to 64. Tags can be up to 64 characters.
                  </p>
                  <textarea
                    ref={detailTagsRef}
                    id="guide-detail-tags"
                    name="guide-detail-tags"
                    autoComplete="off"
                    maxLength={16_384}
                    rows={3}
                    aria-invalid={guideDetailsError !== ""}
                    aria-describedby={
                      guideDetailsError !== ""
                        ? "guide-detail-tags-help guide-details-error"
                        : "guide-detail-tags-help"
                    }
                    value={detailTags}
                    onChange={(event) => {
                      noteDraftChange();
                      setDetailTags(event.target.value);
                      setGuideDetailsError("");
                    }}
                  />
                  <label htmlFor="guide-detail-aliases">Aliases</label>
                  <p className="field-help" id="guide-detail-aliases-help">
                    Add alternate search phrases, one per line, up to 64.
                  </p>
                  <textarea
                    ref={detailAliasesRef}
                    id="guide-detail-aliases"
                    name="guide-detail-aliases"
                    autoComplete="off"
                    maxLength={51_200}
                    rows={3}
                    aria-invalid={guideDetailsError !== ""}
                    aria-describedby={
                      guideDetailsError !== ""
                        ? "guide-detail-aliases-help guide-details-error"
                        : "guide-detail-aliases-help"
                    }
                    value={detailAliases}
                    onChange={(event) => {
                      noteDraftChange();
                      setDetailAliases(event.target.value);
                      setGuideDetailsError("");
                    }}
                  />
                  {selected.status === "Tested" && (
                    <p className="editor-notice">
                      Saving tested Guide details marks the Guide stale until
                      you verify it again.
                    </p>
                  )}
                  {guideDetailsError !== "" && (
                    <p
                      ref={guideDetailsErrorRef}
                      id="guide-details-error"
                      className="editor-error"
                      role="alert"
                      tabIndex={-1}
                    >
                      {guideDetailsError}
                    </p>
                  )}
                  <div className="editor-actions">
                    <button
                      className="button"
                      type="button"
                      disabled={savingGuideDetails}
                      onClick={cancelEditingGuideDetails}
                    >
                      Cancel
                    </button>
                    <button
                      className="button button-primary"
                      type="submit"
                      disabled={savingGuideDetails}
                    >
                      {savingGuideDetails ? "Saving…" : "Save Details"}
                    </button>
                  </div>
                </form>
              ) : (
                <p className="guide-summary">{selected.summary}</p>
              )}
              <dl className="guide-meta">
                <div>
                  <dt>Updated</dt>
                  <dd>{formatLongDate(selected.updatedAt)}</dd>
                </div>
                <div>
                  <dt>Mode</dt>
                  <dd>
                    {editingGuide
                      ? "Editing Content"
                      : editingGuideDetails
                        ? "Editing Details"
                        : "Reader"}
                  </dd>
                </div>
                {selected.tags.length > 0 && (
                  <div>
                    <dt>Tags</dt>
                    <dd>{selected.tags.join(", ")}</dd>
                  </div>
                )}
                {selected.aliases.length > 0 && (
                  <div>
                    <dt>Aliases</dt>
                    <dd>{selected.aliases.join(", ")}</dd>
                  </div>
                )}
              </dl>
            </header>
            {selected.attachments.length > 0 && (
              <section
                className="guide-attachments"
                aria-labelledby="guide-attachments-heading"
              >
                <h2 id="guide-attachments-heading">Attachments</h2>
                <ul>
                  {selected.attachments.map((attachment) => (
                    <li key={attachment.id}>
                      <div className="attachment-info">
                        <span className="attachment-title">
                          {attachment.title}
                        </span>
                        <small className="attachment-size">
                          {formatBytes(attachment.sizeBytes)}
                        </small>
                      </div>
                      {selected.status !== "Archived" && (
                        <button
                          ref={(node) => {
                            if (node) {
                              attachmentRemoveButtonRefs.current.set(
                                attachment.id,
                                node,
                              );
                            } else {
                              attachmentRemoveButtonRefs.current.delete(
                                attachment.id,
                              );
                            }
                          }}
                          className="attachment-remove-button"
                          type="button"
                          aria-label={`Remove ${attachment.title}`}
                          onClick={() => void removeAttachment(attachment.id)}
                          disabled={
                            guideBody?.id !== selected.id ||
                            removingAttachmentId !== "" ||
                            importingAttachment ||
                            verifyingGuide ||
                            updatingGuideStatus ||
                            removingGuide ||
                            editingGuide ||
                            editingGuideDetails
                          }
                        >
                          {removingAttachmentId === attachment.id
                            ? "Removing…"
                            : "Remove"}
                        </button>
                      )}
                    </li>
                  ))}
                </ul>
              </section>
            )}
            <section aria-labelledby="guide-body-heading">
              <h2 className="sr-only" id="guide-body-heading">
                Guide content
              </h2>
              {editingGuide && guideBody?.id === selected.id ? (
                <form
                  className="guide-editor"
                  aria-busy={savingGuide}
                  onSubmit={(event) => {
                    event.preventDefault();
                    void saveGuide();
                  }}
                >
                  <div className="editor-heading">
                    <div>
                      <label htmlFor="guide-markdown">Guide Markdown</label>
                      <p id="guide-markdown-help">
                        Edit the Guide body. Frontmatter and unknown metadata
                        are preserved by Rehow.
                      </p>
                    </div>
                    <span className={dirtyGuide ? "is-dirty" : ""}>
                      {dirtyGuide ? "Unsaved Changes" : "No Changes"}
                    </span>
                  </div>
                  {selected.status === "Tested" && (
                    <p className="editor-notice">
                      Saving a tested Guide marks it stale until you verify it
                      again.
                    </p>
                  )}
                  <textarea
                    ref={editorRef}
                    id="guide-markdown"
                    name="guide-markdown"
                    autoComplete="off"
                    aria-describedby="guide-markdown-help"
                    maxLength={2_097_152}
                    value={draftBody}
                    onChange={(event) => {
                      noteDraftChange();
                      setDraftBody(event.target.value);
                    }}
                  />
                  {saveError !== "" && (
                    <p
                      ref={saveErrorRef}
                      className="editor-error"
                      role="alert"
                      tabIndex={-1}
                    >
                      {saveError}
                    </p>
                  )}
                  <div className="editor-actions">
                    <button
                      className="button"
                      type="button"
                      onClick={cancelEditing}
                      disabled={savingGuide}
                    >
                      Cancel
                    </button>
                    <button
                      className="button button-primary"
                      type="submit"
                      disabled={savingGuide}
                    >
                      {savingGuide ? "Saving…" : "Save Guide"}
                    </button>
                  </div>
                </form>
              ) : guideBody?.id === selected.id ? (
                <MarkdownDocument
                  document={guideBody.document}
                  onAnnouncement={setAnnouncement}
                />
              ) : (
                <p className="guide-loading">Loading Guide…</p>
              )}
            </section>
          </article>
        )}
      </main>

      {syncPanelOpen ? (
        <section
          id="git-sync-panel"
          className="git-sync-panel"
          aria-labelledby="git-sync-heading"
          aria-busy={syncOperationBusy}
        >
          <div className="git-sync-content">
            <div className="git-sync-heading">
              <div>
                <h2 id="git-sync-heading">
                  {syncStage === "inspecting" || syncStage === "cancelling"
                    ? "Inspecting Git Upstream"
                    : (syncPresentation?.title ?? "Git Sync Needs Attention")}
                </h2>
                <p>
                  {syncStage === "inspecting"
                    ? "Fetching and validating the configured upstream without changing this Workspace…"
                    : syncStage === "cancelling"
                      ? "Stopping the isolated inspection safely…"
                      : (syncPresentation?.detail ??
                        "Inspect the error, then retry from current Git status.")}
                </p>
              </div>
              <button
                type="button"
                className="panel-close"
                aria-label={
                  syncStage === "inspecting"
                    ? "Cancel Git sync inspection"
                    : "Close Git sync panel"
                }
                disabled={
                  syncStage === "applying" ||
                  syncStage === "recovering" ||
                  syncStage === "cancelling"
                }
                onClick={() => void cancelGitSync()}
              >
                ×
              </button>
            </div>
            {syncInspection !== null ? (
              <dl className="git-sync-summary">
                <div>
                  <dt>Remote</dt>
                  <dd translate="no">{syncInspection.remoteName}</dd>
                </div>
                <div>
                  <dt>Connection</dt>
                  <dd>
                    <span translate="no">HTTPS</span> ·{" "}
                    <span translate="no">{syncInspection.host}</span>
                  </dd>
                </div>
                <div>
                  <dt>Local Changes</dt>
                  <dd>{syncInspection.localChanges ? "Present" : "None"}</dd>
                </div>
              </dl>
            ) : null}
            {syncError !== "" ? (
              <p
                ref={syncErrorRef}
                className="git-action-error"
                role="alert"
                tabIndex={-1}
              >
                {syncError}
              </p>
            ) : null}
            <div className="git-sync-actions">
              <button
                ref={
                  syncPresentation?.canApply || syncPresentation?.canPush
                    ? undefined
                    : syncPrimaryRef
                }
                type="button"
                className="button"
                disabled={
                  syncStage === "applying" ||
                  syncStage === "cancelling" ||
                  syncStage === "recovering"
                }
                onClick={() => void cancelGitSync()}
              >
                {syncStage === "inspecting"
                  ? "Cancel Inspection"
                  : syncStage === "cancelling"
                    ? "Cancelling…"
                    : "Close"}
              </button>
              {syncPresentation?.canApply ? (
                <button
                  ref={syncPrimaryRef}
                  type="button"
                  className="button button-primary"
                  disabled={syncOperationBusy}
                  onClick={() => void applyGitSync()}
                >
                  {syncStage === "applying"
                    ? "Applying…"
                    : "Apply Fast-Forward"}
                </button>
              ) : null}
              {syncPresentation?.canPush ? (
                <button
                  ref={syncPrimaryRef}
                  type="button"
                  className="button button-primary"
                  disabled={syncOperationBusy}
                  onClick={() => void applyGitPush()}
                >
                  {syncStage === "applying" ? "Pushing…" : "Push Commit"}
                </button>
              ) : null}
            </div>
          </div>
        </section>
      ) : null}

      {commitFormOpen ? (
        <section
          id="git-commit-panel"
          className="git-commit-panel"
          aria-labelledby="git-commit-heading"
        >
          <form
            className="git-commit-form"
            aria-busy={commitOperationBusy}
            onSubmit={(event) => {
              if (commitPlan === null) void planGitCommit(event);
              else {
                event.preventDefault();
                void applyGitCommit();
              }
            }}
          >
            <div className="git-commit-heading">
              <div>
                <h2 id="git-commit-heading">
                  {commitPlan === null
                    ? "Create a Git Commit"
                    : "Review Managed Commit"}
                </h2>
                <p>
                  {commitPlan === null
                    ? "Commit Rehow-managed content without including unrelated worktree changes."
                    : "Review the exact managed effect before native confirmation."}
                </p>
              </div>
              <button
                type="button"
                className="panel-close"
                aria-label="Close Git commit form"
                disabled={commitOperationBusy}
                onClick={() => void cancelGitCommit()}
              >
                ×
              </button>
            </div>
            {commitPlan === null ? (
              <>
                <label htmlFor="git-commit-message">Commit message</label>
                <input
                  ref={commitMessageRef}
                  id="git-commit-message"
                  name="git-commit-message"
                  type="text"
                  autoComplete="off"
                  spellCheck={true}
                  maxLength={200}
                  placeholder="Document the Workspace change…"
                  value={commitMessage}
                  aria-invalid={commitError !== ""}
                  aria-describedby={
                    commitError !== ""
                      ? "git-commit-help git-commit-error"
                      : "git-commit-help"
                  }
                  onChange={(event) => {
                    setCommitMessage(event.target.value);
                    setCommitError("");
                  }}
                />
                <p id="git-commit-help" className="field-help">
                  One line, 1–200 characters. Rehow creates an unsigned local
                  commit and does not push.
                </p>
              </>
            ) : (
              <div className="git-commit-review">
                <p className="git-commit-message">“{commitMessage}”</p>
                <dl>
                  <div>
                    <dt>Branch</dt>
                    <dd translate="no">{commitPlan.branch}</dd>
                  </div>
                  <div>
                    <dt>Created</dt>
                    <dd>
                      {new Intl.NumberFormat().format(commitPlan.created)}
                    </dd>
                  </div>
                  <div>
                    <dt>Updated</dt>
                    <dd>
                      {new Intl.NumberFormat().format(commitPlan.updated)}
                    </dd>
                  </div>
                  <div>
                    <dt>Deleted</dt>
                    <dd>
                      {new Intl.NumberFormat().format(commitPlan.deleted)}
                    </dd>
                  </div>
                  <div>
                    <dt>Unrelated Changes</dt>
                    <dd>
                      {new Intl.NumberFormat().format(
                        commitPlan.unrelatedChanges,
                      )}
                    </dd>
                  </div>
                </dl>
                <p className="field-help">
                  Unrelated changes stay uncommitted. Native confirmation is
                  required before Git metadata changes.
                </p>
              </div>
            )}
            {commitError !== "" ? (
              <p
                ref={commitErrorRef}
                id="git-commit-error"
                className="git-action-error"
                role="alert"
                tabIndex={-1}
              >
                {commitError}
              </p>
            ) : null}
            <div className="git-commit-actions">
              <button
                type="button"
                className="button"
                disabled={commitOperationBusy}
                onClick={() => void cancelGitCommit()}
              >
                {commitPlan === null ? "Cancel" : "Cancel Plan"}
              </button>
              <button
                ref={commitPlan === null ? undefined : commitApplyRef}
                type="submit"
                className="button button-primary"
                disabled={commitOperationBusy}
              >
                {commitStage === "planning"
                  ? "Planning…"
                  : commitStage === "applying"
                    ? "Committing…"
                    : commitPlan === null
                      ? "Review Commit"
                      : "Create Commit"}
              </button>
            </div>
          </form>
        </section>
      ) : null}

      {remoteFormOpen ? (
        <section
          id="git-remote-panel"
          className="git-remote-panel"
          aria-labelledby="git-remote-heading"
        >
          <form
            className="git-remote-form"
            aria-busy={connectingRemote}
            onSubmit={(event) => void connectRemote(event)}
          >
            <div className="git-remote-heading">
              <div>
                <h2 id="git-remote-heading">Connect a Git Remote</h2>
                <p>Rehow updates config only and does not connect.</p>
              </div>
              <button
                type="button"
                className="panel-close"
                aria-label="Close Git remote form"
                disabled={connectingRemote}
                onClick={cancelRemoteConnection}
              >
                ×
              </button>
            </div>
            <label htmlFor="git-remote-name">Remote name</label>
            <input
              ref={remoteNameRef}
              id="git-remote-name"
              name="git-remote-name"
              autoComplete="off"
              spellCheck={false}
              maxLength={64}
              value={remoteName}
              aria-invalid={remoteError !== ""}
              aria-describedby={
                remoteError !== "" ? "git-remote-error" : undefined
              }
              onChange={(event) => {
                setRemoteName(event.target.value);
                setRemoteError("");
              }}
            />
            <label htmlFor="git-remote-url">Repository address</label>
            <input
              id="git-remote-url"
              name="git-remote-url"
              type="text"
              inputMode="url"
              autoComplete="off"
              autoCapitalize="none"
              spellCheck={false}
              maxLength={2048}
              placeholder="https://host/owner/repository.git…"
              translate="no"
              value={remoteUrl}
              aria-invalid={remoteError !== ""}
              aria-describedby={
                remoteError !== ""
                  ? "git-remote-help git-remote-error"
                  : "git-remote-help"
              }
              onChange={(event) => {
                setRemoteUrl(event.target.value);
                setRemoteError("");
              }}
            />
            <p id="git-remote-help" className="field-help">
              HTTPS, ssh://, or user@host:path. Do not include passwords or
              tokens.
            </p>
            {remoteError !== "" ? (
              <p
                ref={remoteErrorRef}
                id="git-remote-error"
                className="git-remote-error"
                role="alert"
                tabIndex={-1}
              >
                {remoteError}
              </p>
            ) : null}
            <div className="git-remote-actions">
              <button
                type="button"
                className="button"
                disabled={connectingRemote}
                onClick={cancelRemoteConnection}
              >
                Cancel
              </button>
              <button
                type="submit"
                className="button button-primary"
                disabled={connectingRemote}
              >
                {connectingRemote ? "Reviewing…" : "Review Connection"}
              </button>
            </div>
          </form>
        </section>
      ) : null}

      <footer className="status-bar">
        <span>
          <span className="sync-indicator" aria-hidden="true" />
          {resultCountLabel(guides.length)} loaded
        </span>
        <span className="status-path" translate="no">
          {workspace?.name ?? "No Workspace"}
        </span>
        <span className="git-status">
          {workspace
            ? gitStatus?.kind === "notRepository" &&
              workspace.facts.nestedGitRepository
              ? "Inside existing Git repository"
              : gitStatusLabel(gitStatus)
            : "Read-only"}
          {workspace?.facts.writable &&
          !workspace.facts.nestedGitRepository &&
          gitStatus?.kind === "notRepository" ? (
            <button
              type="button"
              className="status-action"
              onClick={initializeGit}
              disabled={initializingGit}
            >
              {initializingGit ? "Initializing…" : "Initialize Git"}
            </button>
          ) : null}
          {workspace?.facts.writable && gitStatus?.kind === "ready" ? (
            <button
              ref={remoteButtonRef}
              type="button"
              className="status-action"
              aria-expanded={remoteFormOpen}
              aria-controls="git-remote-panel"
              disabled={
                commitFormOpen ||
                commitOperationBusy ||
                syncPanelOpen ||
                syncOperationBusy
              }
              onClick={beginRemoteConnection}
            >
              Connect Remote
            </button>
          ) : null}
          {workspace?.facts.writable && gitStatus?.kind === "ready" ? (
            <button
              ref={commitButtonRef}
              type="button"
              className="status-action"
              aria-expanded={commitFormOpen}
              aria-controls="git-commit-panel"
              disabled={
                remoteFormOpen ||
                syncPanelOpen ||
                syncOperationBusy ||
                commitOperationBusy ||
                workspaceOperationBusy ||
                dirtyGuide ||
                dirtyGuideDetails ||
                dirtyCategoryCreate ||
                dirtyCategoryRename ||
                dirtyGuideCreate
              }
              onClick={beginGitCommit}
            >
              Commit Changes
            </button>
          ) : null}
          {workspace?.facts.writable &&
          gitStatus?.kind === "ready" &&
          gitStatus.upstream !== null ? (
            <button
              ref={syncButtonRef}
              type="button"
              className="status-action"
              aria-expanded={syncPanelOpen}
              aria-controls="git-sync-panel"
              disabled={
                syncPanelOpen ||
                syncOperationBusy ||
                remoteFormOpen ||
                connectingRemote ||
                commitFormOpen ||
                commitOperationBusy ||
                workspaceOperationBusy ||
                editingGuide ||
                editingGuideDetails ||
                creatingCategory ||
                renamingCategoryId !== "" ||
                creatingGuide ||
                dirtyGuide ||
                dirtyGuideDetails ||
                dirtyCategoryCreate ||
                dirtyCategoryRename ||
                dirtyGuideCreate
              }
              onClick={() => void beginGitSync()}
            >
              {syncStage === "inspecting" ? "Inspecting…" : "Sync"}
            </button>
          ) : null}
          {workspace?.facts.writable &&
          gitStatus?.kind === "recoveryRequired" ? (
            <button
              type="button"
              className="status-action"
              disabled={commitOperationBusy || syncOperationBusy}
              onClick={() => void recoverGitOperation()}
            >
              {syncStage === "recovering"
                ? "Recovering…"
                : "Resume Git Operation"}
            </button>
          ) : null}
          {gitInitError !== "" ? (
            <span className="git-action-error" role="alert">
              {gitInitError}
            </span>
          ) : null}
        </span>
      </footer>
      <div className="sr-only" aria-live="polite">
        {announcement}
      </div>
    </div>
  );
}

function RailItem({
  icon,
  label,
  count,
  active = false,
}: {
  icon: IconName;
  label: string;
  count: number;
  active?: boolean;
}) {
  return (
    <button className={`rail-item ${active ? "is-active" : ""}`} type="button">
      <Icon name={icon} />
      <span>{label}</span>
      <span className="count">{count}</span>
    </button>
  );
}
