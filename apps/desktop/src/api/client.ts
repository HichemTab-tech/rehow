// Typed client for every native Rehow command. Components never call `invoke`
// directly: each function here names the command, passes typed arguments, and
// validates the response before it reaches application state.
import type { SafeMarkdownDocument } from "@rehow/contracts";
import {
  type AttachmentExportPayload,
  type AttachmentTextPreviewPayload,
  type CreateCategoryPayload,
  type CreateGuidePayload,
  type DesktopPreferencesPayload,
  type GitCloneApplyResultPayload,
  type GitCloneCancelResultPayload,
  type GitCloneCompleteResultPayload,
  type GitClonePlanPayload,
  type GitCommitCancelResultPayload,
  type GitCommitPlanPayload,
  type GitCommitRecoveryResultPayload,
  type GitCommitResultPayload,
  type GitIdentityPlanPayload,
  type GitIdentityResultPayload,
  type GitInitPlanPayload,
  type GitInitResultPayload,
  type GitPushResultPayload,
  type GitRemotePlanPayload,
  type GitRemoteResultPayload,
  type GitStatusPayload,
  type GitSyncApplyResultPayload,
  type GitSyncCancelResultPayload,
  type GitSyncInspectionPayload,
  type GuideBodyPayload,
  isAttachmentExportPayload,
  isAttachmentRevealPayload,
  isAttachmentTextPreviewPayload,
  isDesktopPreferencesPayload,
  isExternalLinkOpenPayload,
  isGitCloneApplyResultPayload,
  isGitCloneCancelResultPayload,
  isGitCloneCompleteResultPayload,
  isGitClonePlanPayload,
  isGitCommitCancelResultPayload,
  isGitCommitPlanPayload,
  isGitCommitRecoveryResultPayload,
  isGitCommitResultPayload,
  isGitIdentityPlanPayload,
  isGitIdentityResultPayload,
  isGitInitPlanPayload,
  isGitInitResultPayload,
  isGitPushResultPayload,
  isGitRemotePlanPayload,
  isGitRemoteResultPayload,
  isGitStatusPayload,
  isGitSyncApplyResultPayload,
  isGitSyncCancelResultPayload,
  isGitSyncInspectionPayload,
  isMarkdownImportApplyPayload,
  isMarkdownImportPreviewPayload,
  isRecentWorkspaceResult,
  isSearchHitPayload,
  isWorkspacePayload,
  isWorkspaceWatchEvent,
  type MarkdownImportApplyPayload,
  type MarkdownImportDestinationResult,
  type OpenWorkspaceResult,
  type RecentWorkspaceResult,
  type RemoveGuidePayload,
  type SaveCategoryPayload,
  type SaveGuidePayload,
  type SearchHitPayload,
  type SearchScope,
  type WorkspacePayload,
  type WorkspaceWatchEventPayload,
} from "./payloads";

type Invoke = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;
type Listen = (
  event: string,
  handler: (payload: unknown) => void,
) => Promise<() => void>;

export const isNativeRuntime =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let transport: Promise<{ invoke: Invoke; listen: Listen }> | null = null;

function getTransport() {
  transport ??= isNativeRuntime
    ? Promise.all([
        import("@tauri-apps/api/core"),
        import("@tauri-apps/api/event"),
      ]).then(([core, event]) => ({
        invoke: (command: string, args?: Record<string, unknown>) =>
          core.invoke<unknown>(command, args),
        listen: (name: string, handler: (payload: unknown) => void) =>
          event.listen<unknown>(name, ({ payload }) => handler(payload)),
      }))
    : import.meta.env.DEV
      ? import("./mock").then((mock) => mock.createMockTransport())
      : Promise.reject(
          new NativeError(
            "nativeUnavailable",
            "Rehow must run as the desktop application.",
          ),
        );
  return transport;
}

/** An error raised by a native command, normalized to a code and a message. */
export class NativeError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "NativeError";
  }
}

export function toNativeError(error: unknown): NativeError {
  if (error instanceof NativeError) return error;
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof error.code === "string" &&
    typeof error.message === "string"
  ) {
    return new NativeError(error.code, error.message);
  }
  if (typeof error === "string" && error.length > 0 && error.length < 500) {
    return new NativeError("failed", error);
  }
  return new NativeError(
    "unknown",
    "Something went wrong. Try again, or reopen the Workspace.",
  );
}

async function call<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  validate: ((value: unknown) => value is T) | null,
): Promise<T> {
  const { invoke } = await getTransport();
  let result: unknown;
  try {
    result = await invoke(command, args);
  } catch (error) {
    throw toNativeError(error);
  }
  if (validate !== null && !validate(result)) {
    throw new NativeError(
      "invalidResponse",
      "Rehow received an unexpected response. Try again.",
    );
  }
  return result as T;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function hasWorkspace<T>(value: unknown): value is T {
  return isObject(value) && isWorkspacePayload(value["workspace"]);
}

function isSaveGuidePayload(value: unknown): value is SaveGuidePayload {
  return (
    hasWorkspace<SaveGuidePayload>(value) &&
    isObject(value.guide) &&
    typeof value.changed === "boolean"
  );
}

function isOpenWorkspaceResult(value: unknown): value is OpenWorkspaceResult {
  if (!isObject(value) || typeof value["kind"] !== "string") return false;
  switch (value["kind"]) {
    case "cancelled":
      return true;
    case "compatible":
      return isWorkspacePayload(value["workspace"]);
    case "ordinaryMarkdown":
      return isMarkdownImportPreviewPayload(value["preview"]);
    case "empty":
    case "newerUnsupported":
    case "unrelatedNonempty":
    case "malformed":
      return isObject(value["facts"]);
    default:
      return false;
  }
}

function isSearchHits(value: unknown): value is SearchHitPayload[] {
  return Array.isArray(value) && value.every(isSearchHitPayload);
}

function isMarkdownDocument(value: unknown): value is SafeMarkdownDocument {
  return (
    isObject(value) &&
    typeof value["version"] === "number" &&
    Array.isArray(value["events"])
  );
}

function isGuideBody(value: unknown): value is GuideBodyPayload {
  return (
    isObject(value) &&
    typeof value["id"] === "string" &&
    typeof value["revision"] === "string" &&
    typeof value["body"] === "string" &&
    isMarkdownDocument(value["document"])
  );
}

const WORKSPACE_INVALIDATED_EVENT = "rehow://workspace-invalidated/v1";

export const api = {
  // Session and preferences
  loadPreferences: () =>
    call<DesktopPreferencesPayload>(
      "load_desktop_preferences",
      undefined,
      isDesktopPreferencesPayload,
    ),
  savePreferences: (preferences: DesktopPreferencesPayload) =>
    call<DesktopPreferencesPayload>(
      "save_desktop_preferences",
      preferences,
      isDesktopPreferencesPayload,
    ),
  restoreRecentWorkspace: () =>
    call<RecentWorkspaceResult>(
      "restore_recent_workspace",
      undefined,
      isRecentWorkspaceResult,
    ),
  mcpSetup: () =>
    call<{ workspacePath: string; mcpBinary: string }>(
      "mcp_setup_open_workspace",
      undefined,
      (value): value is { workspacePath: string; mcpBinary: string } =>
        isObject(value) &&
        typeof value["workspacePath"] === "string" &&
        typeof value["mcpBinary"] === "string",
    ),
  rememberWorkspace: () =>
    call<null>("remember_open_workspace", undefined, null),

  // Workspace selection and setup
  chooseWorkspace: () =>
    call<OpenWorkspaceResult>(
      "choose_workspace",
      undefined,
      isOpenWorkspaceResult,
    ),
  initializeWorkspace: (name: string) =>
    call<WorkspacePayload>(
      "initialize_selected_workspace",
      { name },
      isWorkspacePayload,
    ),
  cancelWorkspaceSetup: () =>
    call<null>("cancel_selected_workspace_initialization", undefined, null),
  refreshWorkspace: () =>
    call<WorkspacePayload>(
      "refresh_open_workspace",
      undefined,
      isWorkspacePayload,
    ),
  chooseImportDestination: () =>
    call<MarkdownImportDestinationResult>(
      "choose_markdown_import_destination",
      undefined,
      (value): value is MarkdownImportDestinationResult =>
        isObject(value) &&
        (value["kind"] === "cancelled" || value["kind"] === "selected"),
    ),
  applyMarkdownImport: (workspaceName: string, categoryName: string) =>
    call<MarkdownImportApplyPayload>(
      "apply_markdown_import",
      { workspaceName, categoryName },
      isMarkdownImportApplyPayload,
    ),

  // Reading and search
  readGuide: (guideId: string) =>
    call<GuideBodyPayload>("read_open_guide", { guideId }, isGuideBody),
  readGuideFromDisk: (guideId: string) =>
    call<GuideBodyPayload>(
      "preview_external_open_guide",
      { guideId },
      isGuideBody,
    ),
  search: (query: string, scope: SearchScope) =>
    call<SearchHitPayload[]>(
      "search_open_workspace",
      { query, scope },
      isSearchHits,
    ),
  renderPreview: (body: string) =>
    call<SafeMarkdownDocument>(
      "render_markdown_preview",
      { body },
      isMarkdownDocument,
    ),

  // Categories
  createCategory: (name: string, summary: string | null) =>
    call<CreateCategoryPayload>(
      "create_open_category",
      { name, summary },
      (value): value is CreateCategoryPayload =>
        hasWorkspace<Record<string, unknown>>(value) &&
        typeof value["categoryId"] === "string",
    ),
  saveCategory: (
    categoryId: string,
    expectedRevision: string,
    name: string,
    summary: string | null,
  ) =>
    call<SaveCategoryPayload>(
      "save_open_category_details",
      { categoryId, expectedRevision, name, summary },
      hasWorkspace,
    ),
  setCategoryArchived: (
    categoryId: string,
    expectedRevision: string,
    archived: boolean,
  ) =>
    call<SaveCategoryPayload>(
      "set_open_category_archived",
      { categoryId, expectedRevision, archived },
      hasWorkspace,
    ),
  reorderCategories: (
    ordered: Array<{ id: string; expectedRevision: string }>,
  ) =>
    call<SaveCategoryPayload>(
      "reorder_open_categories",
      { ordered },
      hasWorkspace,
    ),
  removeCategory: (categoryId: string, expectedRevision: string) =>
    call<SaveCategoryPayload>(
      "remove_open_category",
      { categoryId, expectedRevision },
      hasWorkspace,
    ),

  // Guides
  createGuide: (categoryId: string, title: string, summary: string) =>
    call<CreateGuidePayload>(
      "create_open_guide",
      { categoryId, title, summary },
      (value): value is CreateGuidePayload =>
        hasWorkspace<Record<string, unknown>>(value) &&
        isGuideBody(value["guide"]),
    ),
  saveGuideDetails: (
    guideId: string,
    expectedRevision: string,
    details: {
      categoryId: string;
      title: string;
      summary: string;
      tags: string[];
      aliases: string[];
    },
  ) =>
    call<SaveGuidePayload>(
      "save_open_guide_details",
      { guideId, expectedRevision, ...details },
      isSaveGuidePayload,
    ),
  saveGuideBody: (guideId: string, expectedRevision: string, body: string) =>
    call<SaveGuidePayload>(
      "save_open_guide_body",
      { guideId, expectedRevision, body },
      isSaveGuidePayload,
    ),
  setGuideArchived: (
    guideId: string,
    expectedRevision: string,
    archived: boolean,
  ) =>
    call<SaveGuidePayload>(
      "set_open_guide_archived",
      { guideId, expectedRevision, archived },
      isSaveGuidePayload,
    ),
  approveTested: (guideId: string, expectedRevision: string) =>
    call<SaveGuidePayload>(
      "approve_open_guide_tested",
      { guideId, expectedRevision },
      isSaveGuidePayload,
    ),
  removeGuide: (guideId: string, expectedRevision: string) =>
    call<RemoveGuidePayload>(
      "remove_open_guide",
      { guideId, expectedRevision },
      (value): value is RemoveGuidePayload =>
        hasWorkspace<Record<string, unknown>>(value) &&
        typeof value["removed"] === "boolean",
    ),
  openGuideFile: (
    guideId: string,
    expectedRevision: string,
    workspaceSession: string,
  ) =>
    call<{ requested: boolean }>(
      "open_guide_file",
      { guideId, expectedRevision, workspaceSession },
      (value): value is { requested: boolean } =>
        isObject(value) && typeof value["requested"] === "boolean",
    ),
  openExternalLink: (
    guideId: string,
    expectedRevision: string,
    destination: string,
    workspaceSession: string,
  ) =>
    call<{ requested: true }>(
      "open_guide_external_link",
      { guideId, expectedRevision, destination, workspaceSession },
      isExternalLinkOpenPayload,
    ),

  // Attachments
  importAttachment: (guideId: string, expectedRevision: string) =>
    call<SaveGuidePayload>(
      "import_open_guide_attachment",
      { guideId, expectedRevision },
      isSaveGuidePayload,
    ),
  removeAttachment: (
    guideId: string,
    attachmentId: string,
    expectedRevision: string,
  ) =>
    call<SaveGuidePayload>(
      "remove_open_guide_attachment",
      { guideId, attachmentId, expectedRevision },
      isSaveGuidePayload,
    ),
  previewAttachmentText: (
    guideId: string,
    attachmentId: string,
    expectedRevision: string,
    workspaceSession: string,
  ) =>
    call<AttachmentTextPreviewPayload>(
      "preview_open_guide_attachment_text",
      { guideId, attachmentId, expectedRevision, workspaceSession },
      isAttachmentTextPreviewPayload,
    ),
  exportAttachment: (
    guideId: string,
    attachmentId: string,
    expectedRevision: string,
    workspaceSession: string,
  ) =>
    call<AttachmentExportPayload>(
      "export_open_guide_attachment",
      { guideId, attachmentId, expectedRevision, workspaceSession },
      isAttachmentExportPayload,
    ),
  revealAttachment: (
    guideId: string,
    attachmentId: string,
    expectedRevision: string,
    workspaceSession: string,
  ) =>
    call<{ revealed: true }>(
      "reveal_open_guide_attachment_parent",
      { guideId, attachmentId, expectedRevision, workspaceSession },
      isAttachmentRevealPayload,
    ),

  // Git
  git: {
    status: () =>
      call<GitStatusPayload>(
        "git_status_open_workspace",
        undefined,
        isGitStatusPayload,
      ),
    initPlan: () =>
      call<GitInitPlanPayload>(
        "git_initialize_plan_open_workspace",
        undefined,
        isGitInitPlanPayload,
      ),
    initApply: (planToken: string) =>
      call<GitInitResultPayload>(
        "git_initialize_apply_open_workspace",
        { planToken },
        isGitInitResultPayload,
      ),
    remotePlan: (remoteName: string, remoteUrl: string) =>
      call<GitRemotePlanPayload>(
        "git_remote_plan_open_workspace",
        { remoteName, remoteUrl },
        isGitRemotePlanPayload,
      ),
    remoteApply: (planToken: string) =>
      call<GitRemoteResultPayload>(
        "git_remote_apply_open_workspace",
        { planToken },
        isGitRemoteResultPayload,
      ),
    identityManualPlan: (name: string, email: string) =>
      call<GitIdentityPlanPayload>(
        "git_identity_plan_manual_open_workspace",
        { name, email },
        isGitIdentityPlanPayload,
      ),
    identityGlobalPlan: () =>
      call<GitIdentityPlanPayload>(
        "git_identity_plan_global_open_workspace",
        undefined,
        isGitIdentityPlanPayload,
      ),
    identityApply: (planToken: string) =>
      call<GitIdentityResultPayload>(
        "git_identity_apply_open_workspace",
        { planToken },
        isGitIdentityResultPayload,
      ),
    commitPlan: (message: string) =>
      call<GitCommitPlanPayload>(
        "git_commit_plan_open_workspace",
        { message },
        isGitCommitPlanPayload,
      ),
    commitCancel: (planToken: string) =>
      call<GitCommitCancelResultPayload>(
        "git_commit_cancel_open_workspace",
        { planToken },
        isGitCommitCancelResultPayload,
      ),
    commitApply: (planToken: string) =>
      call<GitCommitResultPayload>(
        "git_commit_apply_open_workspace",
        { planToken },
        isGitCommitResultPayload,
      ),
    commitRecover: () =>
      call<GitCommitRecoveryResultPayload>(
        "git_commit_recover_open_workspace",
        undefined,
        isGitCommitRecoveryResultPayload,
      ),
    syncInspect: () =>
      call<GitSyncInspectionPayload>(
        "git_sync_inspect_open_workspace",
        undefined,
        isGitSyncInspectionPayload,
      ),
    syncCancel: () =>
      call<GitSyncCancelResultPayload>(
        "git_sync_cancel_active",
        undefined,
        isGitSyncCancelResultPayload,
      ),
    syncApply: (planToken: string) =>
      call<GitSyncApplyResultPayload>(
        "git_sync_apply_open_workspace",
        { planToken },
        isGitSyncApplyResultPayload,
      ),
    pushApply: (planToken: string) =>
      call<GitPushResultPayload>(
        "git_push_apply_open_workspace",
        { planToken },
        isGitPushResultPayload,
      ),
    syncRecover: () =>
      call<GitSyncApplyResultPayload>(
        "git_sync_recover_open_workspace",
        undefined,
        isGitSyncApplyResultPayload,
      ),
    clonePlan: (remoteUrl: string) =>
      call<GitClonePlanPayload>(
        "git_clone_plan_selected_workspace",
        { remoteUrl },
        isGitClonePlanPayload,
      ),
    cloneApply: (planToken: string) =>
      call<GitCloneApplyResultPayload>(
        "git_clone_apply_selected_workspace",
        { planToken },
        isGitCloneApplyResultPayload,
      ),
    cloneComplete: () =>
      call<GitCloneCompleteResultPayload>(
        "git_clone_complete_selected_workspace",
        undefined,
        isGitCloneCompleteResultPayload,
      ),
    cloneCancel: () =>
      call<GitCloneCancelResultPayload>(
        "git_clone_cancel_active",
        undefined,
        isGitCloneCancelResultPayload,
      ),
  },

  /** Subscribes to native file-watch invalidations for the open Workspace. */
  onWorkspaceInvalidated: async (
    handler: (event: WorkspaceWatchEventPayload) => void,
  ) => {
    const { listen } = await getTransport();
    return listen(WORKSPACE_INVALIDATED_EVENT, (payload) => {
      if (isWorkspaceWatchEvent(payload)) handler(payload);
    });
  },
};
