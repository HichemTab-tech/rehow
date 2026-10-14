// Application state and the workflows that change it. Components read slices
// with selectors and call actions; no component talks to the backend directly.
import { toast } from "sonner";
import { create } from "zustand";
import { api, toNativeError } from "@/api/client";
import type {
  DesktopPreferencesPayload,
  FolderFacts,
  GitStatusPayload,
  GuideBodyPayload,
  GuideSummary,
  MarkdownImportPreviewPayload,
  SearchHitPayload,
  SearchScope,
  WorkspacePayload,
} from "@/api/payloads";
import { confirm } from "@/lib/confirm";
import { plural } from "@/lib/format";

export type SortOrder = "updated" | "title" | "status";

export type Setup =
  | { kind: "none" }
  | { kind: "emptyFolder"; facts: FolderFacts; intent: "create" | "clone" }
  | {
      kind: "import";
      preview: MarkdownImportPreviewPayload;
      destination: FolderFacts | null;
    }
  | { kind: "notice"; title: string; message: string };

export type Draft = {
  guideId: string;
  title: string;
  summary: string;
  categoryId: string;
  tags: string[];
  aliases: string[];
  body: string;
};

export type Dialog =
  | null
  | "palette"
  | "settings"
  | "issues"
  | "newGuide"
  | "newCategory"
  | "shortcuts"
  | "git";

type State = {
  booting: boolean;
  workspace: WorkspacePayload | null;
  setup: Setup;
  preferences: DesktopPreferencesPayload;

  query: string;
  scope: SearchScope;
  categoryFilter: string | null;
  tagFilter: string | null;
  sort: SortOrder;
  hits: SearchHitPayload[] | null;
  hitsQuery: string;

  selectedId: string | null;
  guide: GuideBodyPayload | null;
  guideLoading: boolean;

  draft: Draft | null;
  saving: boolean;

  /** Files changed on disk while the user had unsaved work. */
  externalChange: boolean;
  gitStatus: GitStatusPayload | null;

  dialog: Dialog;
  sidebarOpen: boolean;
  /** Label of a running workspace-wide operation, shown as busy state. */
  busy: string | null;
  /** Incremented when the user explicitly opens a Guide (narrow layouts show the reader). */
  readerRequest: number;
};

const initial: State = {
  booting: true,
  workspace: null,
  setup: { kind: "none" },
  preferences: { syncOnStart: false, syncIntervalMinutes: null },
  query: "",
  scope: "active",
  categoryFilter: null,
  tagFilter: null,
  sort: "updated",
  hits: null,
  hitsQuery: "",
  selectedId: null,
  guide: null,
  guideLoading: false,
  draft: null,
  saving: false,
  externalChange: false,
  gitStatus: null,
  dialog: null,
  sidebarOpen: true,
  busy: null,
  readerRequest: 0,
};

export const useApp = create<State>(() => initial);
const set = useApp.setState;
const get = useApp.getState;

export function reportError(error: unknown, prefix?: string) {
  const failure = toNativeError(error);
  toast.error(
    prefix ?? failure.message,
    prefix ? { description: failure.message } : undefined,
  );
  return failure;
}

// ---------------------------------------------------------------- derived ---

export function draftIsDirty(state: State = get()): boolean {
  const { draft, guide, workspace } = state;
  if (draft === null || guide === null) return false;
  const summary = workspace?.guides.find((item) => item.id === draft.guideId);
  return (
    draft.body !== guide.body ||
    draft.title !== guide.title ||
    draft.summary !== guide.summary ||
    draft.categoryId !== (summary?.categoryId ?? guide.categoryId) ||
    draft.tags.join("\n") !== guide.tags.join("\n") ||
    draft.aliases.join("\n") !== guide.aliases.join("\n")
  );
}

export function selectedSummary(state: State = get()): GuideSummary | null {
  return (
    state.workspace?.guides.find((item) => item.id === state.selectedId) ?? null
  );
}

function matchesScope(status: GuideSummary["status"], scope: SearchScope) {
  if (scope === "all") return true;
  if (scope === "active") return status !== "Archived";
  return status.toLowerCase() === scope;
}

const STATUS_RANK: Record<GuideSummary["status"], number> = {
  Stale: 0,
  Draft: 1,
  Tested: 2,
  Archived: 3,
};

/** The Guides shown in the list, after search, scope, filters and sorting. */
export function visibleGuides(
  state: Pick<
    State,
    | "workspace"
    | "query"
    | "scope"
    | "categoryFilter"
    | "tagFilter"
    | "sort"
    | "hits"
  >,
): GuideSummary[] {
  const guides = state.workspace?.guides ?? [];
  const scoped = guides.filter(
    (guide) =>
      matchesScope(guide.status, state.scope) &&
      (state.categoryFilter === null ||
        guide.categoryId === state.categoryFilter) &&
      (state.tagFilter === null || guide.tags.includes(state.tagFilter)),
  );
  const query = state.query.trim();
  if (query !== "") {
    if (state.hits !== null) {
      const byId = new Map(scoped.map((guide) => [guide.id, guide]));
      return state.hits.flatMap((hit) => byId.get(hit.guideId) ?? []);
    }
    const terms = query.toLocaleLowerCase().split(/\s+/);
    return scoped.filter((guide) => {
      const text =
        `${guide.title} ${guide.summary} ${guide.category} ${guide.matches}`.toLocaleLowerCase();
      return terms.every((term) => text.includes(term));
    });
  }
  const sorted = [...scoped];
  if (state.sort === "title")
    sorted.sort((a, b) => a.title.localeCompare(b.title));
  else if (state.sort === "status")
    sorted.sort(
      (a, b) =>
        STATUS_RANK[a.status] - STATUS_RANK[b.status] ||
        b.updatedAt.localeCompare(a.updatedAt),
    );
  else sorted.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  return sorted;
}

// ------------------------------------------------------------ navigation ---

/** Asks before throwing away unsaved Guide edits. Resolves true when safe. */
export async function confirmLeaveDraft(): Promise<boolean> {
  if (!draftIsDirty()) {
    if (get().draft !== null) set({ draft: null });
    return true;
  }
  const discard = await confirm({
    title: "Discard unsaved changes?",
    description: "Your edits to this Guide have not been saved.",
    confirmLabel: "Discard",
    cancelLabel: "Keep editing",
    destructive: true,
  });
  if (discard) set({ draft: null });
  return discard;
}

let guideRequest = 0;

export async function selectGuide(
  guideId: string | null,
  options?: { force?: boolean; open?: boolean },
) {
  const state = get();
  if (options?.open) set({ readerRequest: state.readerRequest + 1 });
  if (guideId === state.selectedId && !options?.force) return;
  if (!(await confirmLeaveDraft())) return;
  set({ selectedId: guideId, draft: null });
  await loadGuide();
}

async function loadGuide() {
  const { selectedId, workspace } = get();
  const request = ++guideRequest;
  if (selectedId === null || workspace === null) {
    set({ guide: null, guideLoading: false });
    return;
  }
  set({ guideLoading: true });
  try {
    const guide = await api.readGuide(selectedId);
    if (request === guideRequest) set({ guide, guideLoading: false });
  } catch (error) {
    if (request !== guideRequest) return;
    set({ guide: null, guideLoading: false });
    reportError(error, "This Guide could not be opened");
  }
}

// -------------------------------------------------------------- workspace ---

function installWorkspace(
  workspace: WorkspacePayload,
  options: { reset: boolean },
) {
  const current = get();
  const keepSelection =
    !options.reset &&
    current.selectedId !== null &&
    workspace.guides.some((guide) => guide.id === current.selectedId);
  const keepCategory =
    !options.reset &&
    current.categoryFilter !== null &&
    workspace.categories.some(
      (category) => category.id === current.categoryFilter,
    );
  set({
    workspace,
    setup: { kind: "none" },
    externalChange: false,
    selectedId: keepSelection
      ? current.selectedId
      : (workspace.guides[0]?.id ?? null),
    categoryFilter: keepCategory ? current.categoryFilter : null,
    tagFilter: options.reset ? null : current.tagFilter,
    query: options.reset ? "" : current.query,
    hits: options.reset ? null : current.hits,
    draft: options.reset ? null : current.draft,
  });
  if (options.reset) void api.rememberWorkspace().catch(() => undefined);
  void loadGuide();
  void refreshGitStatus();
}

let booted = false;

export async function boot() {
  // React StrictMode mounts effects twice in development; start only once.
  if (booted) return;
  booted = true;
  try {
    const preferences = await api.loadPreferences();
    set({ preferences });
  } catch {
    // Preferences are optional; defaults stay in place.
  }
  try {
    const recent = await api.restoreRecentWorkspace();
    if (recent.kind === "restored")
      installWorkspace(recent.workspace, { reset: true });
    else if (recent.kind === "unavailable")
      toast.warning("Your last Workspace could not be opened", {
        description: recent.reason,
      });
  } catch (error) {
    reportError(error);
  } finally {
    set({ booting: false });
  }
}

/**
 * Opens the native folder picker and routes on what the folder contains.
 * `intent` only decides which follow-up form appears for an empty folder.
 */
export async function openWorkspace(
  intent: "open" | "create" | "clone" = "open",
) {
  if (!(await confirmLeaveDraft())) return;
  set({ busy: "Choosing folder…" });
  try {
    const result = await api.chooseWorkspace();
    switch (result.kind) {
      case "cancelled":
        return;
      case "compatible":
        installWorkspace(result.workspace, { reset: true });
        toast.success(`Opened ${result.workspace.name}`, {
          description: plural(result.workspace.guides.length, "Guide"),
        });
        return;
      case "empty":
        set({
          workspace: null,
          selectedId: null,
          guide: null,
          setup: {
            kind: "emptyFolder",
            facts: result.facts,
            intent: intent === "clone" ? "clone" : "create",
          },
        });
        return;
      case "ordinaryMarkdown":
        set({
          workspace: null,
          selectedId: null,
          guide: null,
          setup: { kind: "import", preview: result.preview, destination: null },
        });
        return;
      case "newerUnsupported":
        set({
          setup: {
            kind: "notice",
            title: "Made by a newer Rehow",
            message: `This Workspace uses format version ${result.version}. Update Rehow to open it.`,
          },
        });
        return;
      case "unrelatedNonempty":
        set({
          setup: {
            kind: "notice",
            title: "That folder isn't a Workspace",
            message:
              "It has files but no Markdown Guides. Choose an empty folder to create a new Workspace, or a folder of Markdown notes to import.",
          },
        });
        return;
      case "malformed":
        set({
          setup: {
            kind: "notice",
            title: "This Workspace couldn't be read",
            message: result.reason,
          },
        });
        return;
    }
  } catch (error) {
    reportError(error, "Couldn't open the folder");
  } finally {
    set({ busy: null });
  }
}

export async function cancelSetup() {
  await api.cancelWorkspaceSetup().catch(() => undefined);
  set({ setup: { kind: "none" } });
}

export async function createWorkspace(name: string) {
  set({ busy: "Creating Workspace…" });
  try {
    const workspace = await api.initializeWorkspace(name);
    installWorkspace(workspace, { reset: true });
    toast.success(`${workspace.name} is ready`, {
      description: "Create a Category, then add your first Guide.",
    });
    return true;
  } catch (error) {
    reportError(error, "Couldn't create the Workspace");
    return false;
  } finally {
    set({ busy: null });
  }
}

export function adoptWorkspace(workspace: WorkspacePayload, message?: string) {
  installWorkspace(workspace, { reset: true });
  if (message) toast.success(message);
}

export async function refreshWorkspace(options: { automatic?: boolean } = {}) {
  const { workspace } = get();
  if (workspace === null) return;
  if (options.automatic && (draftIsDirty() || get().saving)) {
    set({ externalChange: true });
    return;
  }
  try {
    const refreshed = await api.refreshWorkspace();
    const previous = get().selectedId;
    installWorkspace(refreshed, { reset: false });
    if (
      previous !== null &&
      !refreshed.guides.some((guide) => guide.id === previous)
    ) {
      toast.info("The open Guide was removed or moved outside Rehow.");
    } else if (!options.automatic) {
      toast.success("Workspace refreshed");
    }
  } catch (error) {
    if (!options.automatic)
      reportError(error, "Couldn't refresh the Workspace");
  }
}

export async function refreshGitStatus() {
  if (get().workspace === null) return;
  try {
    set({ gitStatus: await api.git.status() });
  } catch {
    set({
      gitStatus: {
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
        identity: null,
        globalIdentity: "unsafe",
        failure: "Git status is unavailable.",
        sandbox: null,
      },
    });
  }
}

// ------------------------------------------------------------- filtering ---

export function setQuery(query: string) {
  set({ query });
}

export function setScope(scope: SearchScope) {
  set({ scope, hits: null });
}

export function filterByCategory(categoryId: string | null) {
  set({ categoryFilter: categoryId, tagFilter: null, scope: "active" });
}

export function filterByTag(tag: string | null) {
  set({ tagFilter: tag, categoryFilter: null, scope: "active" });
}

export function clearFilters() {
  set({
    query: "",
    hits: null,
    categoryFilter: null,
    tagFilter: null,
    scope: "active",
  });
}

// ---------------------------------------------------------------- editing ---

export function startEditing() {
  const { guide, workspace } = get();
  if (guide === null) return;
  const summary = workspace?.guides.find((item) => item.id === guide.id);
  set({
    draft: {
      guideId: guide.id,
      title: guide.title,
      summary: guide.summary,
      categoryId: summary?.categoryId ?? guide.categoryId,
      tags: [...guide.tags],
      aliases: [...guide.aliases],
      body: guide.body,
    },
  });
}

export function updateDraft(patch: Partial<Omit<Draft, "guideId">>) {
  const { draft } = get();
  if (draft !== null) set({ draft: { ...draft, ...patch } });
}

export async function cancelEditing() {
  await confirmLeaveDraft();
}

function validateDraft(draft: Draft): string | null {
  const title = draft.title.trim();
  if (title === "") return "Give the Guide a title.";
  if ([...title].length > 200) return "Keep the title under 200 characters.";
  const summary = draft.summary.trim();
  if ([...summary].length > 500)
    return "Keep the summary under 500 characters.";
  if (draft.categoryId === "") return "Choose a Category.";
  if (draft.tags.some((tag) => [...tag].length > 64))
    return "Tags must be under 64 characters.";
  if (draft.tags.length > 64 || draft.aliases.length > 64)
    return "Use at most 64 tags and 64 aliases.";
  return null;
}

/** Saves details and content as needed. Returns true when everything saved. */
export async function saveDraft(options: { keepEditing?: boolean } = {}) {
  const { draft, guide, saving } = get();
  if (draft === null || guide === null || saving) return false;
  const problem = validateDraft(draft);
  if (problem !== null) {
    toast.error(problem);
    return false;
  }
  const wasTested =
    get().workspace?.guides.find((item) => item.id === guide.id)?.status ===
    "Tested";
  const adopting = guide.origin === "unadopted";
  // The file format needs a summary; fall back to the title, as adoption does.
  const summaryToSave = draft.summary.trim() || draft.title.trim();
  set({ saving: true });
  try {
    let current = guide;
    let workspace = get().workspace;
    let changed = false;
    const detailsChanged =
      draft.title.trim() !== current.title ||
      summaryToSave !== current.summary ||
      draft.categoryId !==
        (workspace?.guides.find((item) => item.id === current.id)?.categoryId ??
          current.categoryId) ||
      draft.tags.join("\n") !== current.tags.join("\n") ||
      draft.aliases.join("\n") !== current.aliases.join("\n");
    if (detailsChanged) {
      const result = await api.saveGuideDetails(current.id, current.revision, {
        categoryId: draft.categoryId,
        title: draft.title.trim(),
        summary: summaryToSave,
        tags: draft.tags,
        aliases: draft.aliases,
      });
      current = result.guide;
      workspace = result.workspace;
      changed ||= result.changed;
    }
    if (draft.body !== current.body) {
      const result = await api.saveGuideBody(
        current.id,
        current.revision,
        draft.body,
      );
      current = result.guide;
      workspace = result.workspace;
      changed ||= result.changed;
    }
    // Adopting a hand-written Guide gives it a new, permanent id.
    set({
      guide: current,
      selectedId: current.id,
      workspace: workspace ?? get().workspace,
      draft: options.keepEditing
        ? {
            ...draft,
            guideId: current.id,
            title: current.title,
            summary: current.summary,
            tags: [...current.tags],
            aliases: [...current.aliases],
            body: current.body,
          }
        : null,
    });
    clearStoredDraft(draft.guideId);
    if (changed) {
      toast.success(
        adopting ? "Saved — Rehow now manages this file" : "Saved",
        {
          description: adopting
            ? "Metadata was added at the top of the file. Its name and content are unchanged."
            : wasTested
              ? "The Guide is now marked “Needs retest” until you confirm it works again."
              : undefined,
        },
      );
      void refreshGitStatus();
    }
    return true;
  } catch (error) {
    const failure = toNativeError(error);
    if (/revision|conflict|changed/i.test(failure.code + failure.message)) {
      set({ externalChange: true });
      toast.error("This Guide changed on disk", {
        description: "Compare your draft with the file before saving again.",
      });
    } else {
      reportError(error, "Couldn't save the Guide");
    }
    return false;
  } finally {
    set({ saving: false });
  }
}

// Unsaved drafts are mirrored to local storage so a crash or reload never
// loses work. They are restored only when the Guide's content still matches.
const DRAFT_KEY = "rehow:draft:";

export function storeDraft(draft: Draft, baseRevision: string) {
  try {
    localStorage.setItem(
      DRAFT_KEY + draft.guideId,
      JSON.stringify({ draft, baseRevision }),
    );
  } catch {
    // Storage is a convenience only.
  }
}

export function clearStoredDraft(guideId: string) {
  try {
    localStorage.removeItem(DRAFT_KEY + guideId);
  } catch {
    // Storage is a convenience only.
  }
}

export function readStoredDraft(
  guideId: string,
): { draft: Draft; baseRevision: string } | null {
  try {
    const raw = localStorage.getItem(DRAFT_KEY + guideId);
    return raw === null
      ? null
      : (JSON.parse(raw) as { draft: Draft; baseRevision: string });
  } catch {
    return null;
  }
}

// ---------------------------------------------------------- guide actions ---

function applySavedGuide(result: {
  workspace: WorkspacePayload;
  guide: GuideBodyPayload;
}) {
  set({ workspace: result.workspace, guide: result.guide });
  void refreshGitStatus();
}

export async function createGuide(input: {
  title: string;
  summary: string;
  categoryId: string;
}) {
  if (!(await confirmLeaveDraft())) return false;
  try {
    const result = await api.createGuide(
      input.categoryId,
      input.title,
      input.summary,
    );
    set({
      workspace: result.workspace,
      selectedId: result.guide.id,
      guide: result.guide,
      query: "",
      hits: null,
      scope: "active",
      tagFilter: null,
      categoryFilter: get().categoryFilter === null ? null : input.categoryId,
    });
    startEditing();
    void refreshGitStatus();
    return true;
  } catch (error) {
    reportError(error, "Couldn't create the Guide");
    return false;
  }
}

export async function setGuideArchived(archived: boolean) {
  const { guide } = get();
  if (guide === null || !(await confirmLeaveDraft())) return;
  try {
    const result = await api.setGuideArchived(
      guide.id,
      guide.revision,
      archived,
    );
    applySavedGuide(result);
    toast.success(archived ? "Guide archived" : "Guide restored", {
      action: {
        label: "Undo",
        onClick: () => void setGuideArchived(!archived),
      },
    });
  } catch (error) {
    reportError(
      error,
      archived ? "Couldn't archive the Guide" : "Couldn't restore the Guide",
    );
  }
}

export async function markTested() {
  const { guide } = get();
  if (guide === null || !(await confirmLeaveDraft())) return;
  try {
    const result = await api.approveTested(guide.id, guide.revision);
    if (!result.changed) return;
    applySavedGuide(result);
    toast.success("Marked as tested", {
      description: "You confirmed these steps work as written.",
    });
  } catch (error) {
    reportError(error, "Couldn't mark the Guide as tested");
  }
}

export async function deleteGuide() {
  const { guide } = get();
  if (guide === null) return;
  const ok = await confirm({
    title: `Delete “${guide.title}”?`,
    description:
      "The file moves to the Workspace trash (.rehow/trash), so it can still be recovered from disk.",
    confirmLabel: "Delete Guide",
    destructive: true,
  });
  if (!ok) return;
  set({ draft: null });
  try {
    const result = await api.removeGuide(guide.id, guide.revision);
    if (!result.removed) return;
    const list = visibleGuides(get());
    const index = list.findIndex((item) => item.id === guide.id);
    const next = list[index + 1] ?? list[index - 1] ?? null;
    set({
      workspace: result.workspace,
      selectedId: next?.id ?? null,
      guide: null,
    });
    void loadGuide();
    void refreshGitStatus();
    toast.success("Guide deleted");
  } catch (error) {
    reportError(error, "Couldn't delete the Guide");
  }
}

export async function openGuideFile() {
  const { guide, workspace } = get();
  if (guide === null || workspace === null) return;
  try {
    await api.openGuideFile(guide.id, guide.revision, workspace.watchSession);
  } catch (error) {
    reportError(error, "Couldn't open the file");
  }
}

export async function openExternalLink(destination: string) {
  const { guide, workspace } = get();
  if (guide === null || workspace === null) return;
  try {
    await api.openExternalLink(
      guide.id,
      guide.revision,
      destination,
      workspace.watchSession,
    );
  } catch (error) {
    reportError(error, "Couldn't open the link");
  }
}

export async function importAttachment() {
  const { guide } = get();
  if (guide === null || !(await confirmLeaveDraft())) return;
  try {
    const result = await api.importAttachment(guide.id, guide.revision);
    if (result.changed) {
      applySavedGuide(result);
      toast.success("Attachment added");
    }
  } catch (error) {
    reportError(error, "Couldn't add the attachment");
  }
}

export async function removeAttachment(attachmentId: string, title: string) {
  const { guide } = get();
  if (guide === null) return;
  const ok = await confirm({
    title: `Remove “${title}”?`,
    description: "The file moves to the Workspace trash.",
    confirmLabel: "Remove",
    destructive: true,
  });
  if (!ok) return;
  try {
    const result = await api.removeAttachment(
      guide.id,
      attachmentId,
      guide.revision,
    );
    if (result.changed) {
      applySavedGuide(result);
      toast.success("Attachment removed");
    }
  } catch (error) {
    reportError(error, "Couldn't remove the attachment");
  }
}

export async function exportAttachment(attachmentId: string) {
  const { guide, workspace } = get();
  if (guide === null || workspace === null) return;
  try {
    const result = await api.exportAttachment(
      guide.id,
      attachmentId,
      guide.revision,
      workspace.watchSession,
    );
    if (result.kind === "exported") toast.success("Attachment saved");
  } catch (error) {
    reportError(error, "Couldn't save the attachment");
  }
}

export async function revealAttachment(attachmentId: string) {
  const { guide, workspace } = get();
  if (guide === null || workspace === null) return;
  try {
    await api.revealAttachment(
      guide.id,
      attachmentId,
      guide.revision,
      workspace.watchSession,
    );
  } catch (error) {
    reportError(error, "Couldn't show the attachment");
  }
}

// ------------------------------------------------------------- categories ---

function applyCategoryResult(workspace: WorkspacePayload) {
  const current = get();
  set({
    workspace,
    categoryFilter: workspace.categories.some(
      (category) => category.id === current.categoryFilter,
    )
      ? current.categoryFilter
      : null,
  });
  void refreshGitStatus();
}

export async function createCategory(name: string, summary: string | null) {
  try {
    const result = await api.createCategory(name, summary);
    applyCategoryResult(result.workspace);
    toast.success(`Created “${name}”`);
    return result.categoryId;
  } catch (error) {
    reportError(error, "Couldn't create the Category");
    return null;
  }
}

export async function renameCategory(
  categoryId: string,
  name: string,
  summary: string | null,
) {
  const category = get().workspace?.categories.find(
    (item) => item.id === categoryId,
  );
  if (!category) return false;
  try {
    const result = await api.saveCategory(
      categoryId,
      category.revision,
      name,
      summary,
    );
    applyCategoryResult(result.workspace);
    return true;
  } catch (error) {
    reportError(error, "Couldn't rename the Category");
    return false;
  }
}

export async function setCategoryArchived(
  categoryId: string,
  archived: boolean,
) {
  const category = get().workspace?.categories.find(
    (item) => item.id === categoryId,
  );
  if (!category) return;
  try {
    const result = await api.setCategoryArchived(
      categoryId,
      category.revision,
      archived,
    );
    applyCategoryResult(result.workspace);
    toast.success(
      archived ? `Archived “${category.name}”` : `Restored “${category.name}”`,
    );
  } catch (error) {
    reportError(
      error,
      archived
        ? "Couldn't archive the Category"
        : "Couldn't restore the Category",
    );
  }
}

export async function moveCategory(categoryId: string, direction: -1 | 1) {
  const categories = get().workspace?.categories ?? [];
  const index = categories.findIndex((item) => item.id === categoryId);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= categories.length) return;
  const reordered = [...categories];
  const [moved] = reordered.splice(index, 1);
  if (!moved) return;
  reordered.splice(target, 0, moved);
  await reorderCategories(reordered.map((item) => item.id));
}

export async function reorderCategories(orderedIds: string[]) {
  const categories = get().workspace?.categories ?? [];
  const byId = new Map(categories.map((item) => [item.id, item]));
  try {
    const result = await api.reorderCategories(
      orderedIds.flatMap((id) => {
        const category = byId.get(id);
        return category ? [{ id, expectedRevision: category.revision }] : [];
      }),
    );
    applyCategoryResult(result.workspace);
  } catch (error) {
    reportError(error, "Couldn't reorder Categories");
  }
}

export async function deleteCategory(categoryId: string) {
  const category = get().workspace?.categories.find(
    (item) => item.id === categoryId,
  );
  if (!category) return;
  if (category.guideCount > 0) {
    toast.error(
      `“${category.name}” still has ${plural(category.guideCount, "Guide")}`,
      {
        description:
          "Move or delete them first, or archive the Category instead.",
      },
    );
    return;
  }
  const ok = await confirm({
    title: `Delete “${category.name}”?`,
    description: "The empty Category folder moves to the Workspace trash.",
    confirmLabel: "Delete Category",
    destructive: true,
  });
  if (!ok) return;
  try {
    const result = await api.removeCategory(categoryId, category.revision);
    applyCategoryResult(result.workspace);
    toast.success(`Deleted “${category.name}”`);
  } catch (error) {
    reportError(error, "Couldn't delete the Category");
  }
}

// --------------------------------------------------------------------- ui ---

export function openDialog(dialog: Dialog) {
  set({ dialog });
}

export function closeDialog() {
  set({ dialog: null });
}

export function toggleSidebar() {
  set({ sidebarOpen: !get().sidebarOpen });
}
