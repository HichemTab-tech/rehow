import type { SafeMarkdownDocument } from "@rehow/contracts";
import { useEffect, useMemo, useRef, useState } from "react";
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
};

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
  rankedIds: string[] | null,
) {
  if (query.trim() === "") return guides;
  if (rankedIds === null) return filterGuides(guides, query);
  const byId = new Map(guides.map((guide) => [guide.id, guide]));
  return rankedIds.flatMap((id) => {
    const guide = byId.get(id);
    return guide ? [guide] : [];
  });
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

export function App() {
  const [query, setQuery] = useState(() => initialParameter("q") ?? "");
  const [workspace, setWorkspace] = useState<WorkspacePayload | null>(null);
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
  const [pendingWorkspace, setPendingWorkspace] = useState<FolderFacts | null>(
    null,
  );
  const [workspaceName, setWorkspaceName] = useState("");
  const [initializingWorkspace, setInitializingWorkspace] = useState(false);
  const [workspaceSetupError, setWorkspaceSetupError] = useState("");
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
  const [removingGuide, setRemovingGuide] = useState(false);
  const [guideStatusError, setGuideStatusError] = useState("");
  const [rankedGuideIds, setRankedGuideIds] = useState<string[] | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const readerRef = useRef<HTMLElement>(null);
  const resultListRef = useRef<HTMLUListElement>(null);
  const lastResultRef = useRef<HTMLButtonElement | null>(null);
  const railToggleRef = useRef<HTMLButtonElement>(null);
  const workspaceRefreshRef = useRef<HTMLButtonElement>(null);
  const editButtonRef = useRef<HTMLButtonElement>(null);
  const editDetailsButtonRef = useRef<HTMLButtonElement>(null);
  const guideStatusButtonRef = useRef<HTMLButtonElement>(null);
  const guideVerifyButtonRef = useRef<HTMLButtonElement>(null);
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
  const guides = workspace?.guides ?? [];
  const visibleGuides = useMemo(
    () => rankedGuides(guides, query, rankedGuideIds),
    [guides, query, rankedGuideIds],
  );
  const selected =
    guides.find((guide) => guide.id === selectedId) ?? visibleGuides[0] ?? null;
  const dirtyGuide =
    editingGuide &&
    guideBody !== null &&
    guideBody.id === selected?.id &&
    hasUnsavedChanges(guideBody.body, draftBody);
  const dirtyGuideDetails =
    editingGuideDetails &&
    selected !== null &&
    (selected.title !== detailTitle ||
      selected.summary !== detailSummary ||
      selected.categoryId !== detailCategoryId ||
      selected.tags.join("\n") !== detailTags ||
      selected.aliases.join("\n") !== detailAliases);
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
  const workspaceOperationBusy =
    savingGuide ||
    savingNewGuide ||
    savingGuideDetails ||
    savingCategory ||
    savingCategoryRename ||
    updatingGuideStatus ||
    verifyingGuide ||
    updatingCategoryStatusId !== "" ||
    reorderingCategoryId !== "" ||
    removingCategoryId !== "" ||
    removingGuide ||
    refreshingWorkspace;

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
    function handleShortcut(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      const editing = target?.matches(
        "input, textarea, [contenteditable='true']",
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
  }, [editingGuide, editingGuideDetails, railOpen, readerOpen]);

  useEffect(() => {
    if (
      !dirtyGuide &&
      !dirtyGuideDetails &&
      !dirtyCategoryRename &&
      !dirtyCategoryCreate
    )
      return;
    function warnBeforeUnload(event: BeforeUnloadEvent) {
      event.preventDefault();
      event.returnValue = "";
    }
    window.addEventListener("beforeunload", warnBeforeUnload);
    return () => window.removeEventListener("beforeunload", warnBeforeUnload);
  }, [dirtyCategoryCreate, dirtyCategoryRename, dirtyGuide, dirtyGuideDetails]);

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
      setRankedGuideIds(null);
      return;
    }
    let active = true;
    setRankedGuideIds(null);
    const timeout = window.setTimeout(() => {
      import("@tauri-apps/api/core")
        .then(({ invoke }) =>
          invoke<string[]>("search_open_workspace", { query }),
        )
        .then((ids) => {
          if (active) setRankedGuideIds(ids);
        })
        .catch(() => {
          if (active) {
            setRankedGuideIds([]);
            setAnnouncement("Search could not be completed.");
          }
        });
    }, 80);
    return () => {
      active = false;
      window.clearTimeout(timeout);
    };
  }, [query, workspace]);

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
    const dirtyNewGuide =
      creatingGuide && (guideTitle.trim() !== "" || guideSummary.trim() !== "");
    return (
      !dirtyNewGuide || window.confirm("Discard this unfinished Guide draft?")
    );
  }

  function openGuide(id: string, trigger?: HTMLButtonElement) {
    if (id !== selectedId && !confirmDiscard()) return;
    if (trigger) lastResultRef.current = trigger;
    setSelectedId(id);
    setReaderOpen(true);
    requestAnimationFrame(() => readerRef.current?.focus());
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

  async function openWorkspace() {
    if (!confirmDiscard()) return;
    setWorkspaceSetupError("");
    setOpeningWorkspace(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<OpenWorkspaceResult>("choose_workspace");
      if (result.kind !== "cancelled") {
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
          `Empty folder selected. Name the new Workspace to create it. ${folderFactSuffix(result.facts)}`,
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

  async function refreshWorkspace() {
    if (workspace === null || refreshingWorkspace || !confirmDiscard()) return;
    const previousSelectedId = selectedId;
    const previousSelectedTitle = selected?.title ?? "The selected Guide";
    setRefreshingWorkspace(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const refreshed = await invoke<WorkspacePayload>(
        "refresh_open_workspace",
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
          `${previousSelectedTitle} was removed or is unavailable. Workspace refreshed with ${resultCountLabel(refreshed.guides.length)}.`,
        );
        requestAnimationFrame(() => searchRef.current?.focus());
      } else {
        const issueMessage =
          refreshed.issueCount === 0
            ? "No issues found."
            : `${refreshed.issueCount} item${refreshed.issueCount === 1 ? "" : "s"} need attention.`;
        setAnnouncement(
          `Workspace refreshed with ${resultCountLabel(refreshed.guides.length)}. ${issueMessage}`,
        );
        requestAnimationFrame(() => workspaceRefreshRef.current?.focus());
      }
    } catch (error) {
      const failure = nativeCommandError(error);
      setAnnouncement(failure.message);
      requestAnimationFrame(() => workspaceRefreshRef.current?.focus());
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

  async function cancelWorkspaceSetup() {
    if (initializingWorkspace) return;
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
    setAnnouncement("Workspace creation cancelled.");
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

  async function removeGuide() {
    if (
      selected === null ||
      guideBody?.id !== selected.id ||
      removingGuide ||
      updatingGuideStatus ||
      verifyingGuide ||
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
          disabled={openingWorkspace}
        >
          <span className="workspace-dot" aria-hidden="true" />
          {openingWorkspace
            ? "Opening…"
            : (workspace?.name ?? "Open Workspace")}
          <Icon name="chevron" />
        </button>
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
                  : "Refresh Workspace"
            }
            disabled={
              workspace === null || workspaceOperationBusy || openingWorkspace
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
            <button type="button">
              Recently Updated <span aria-hidden="true">⌄</span>
            </button>
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
            visibleGuides.map((guide, index) => (
              <li key={guide.id}>
                <button
                  type="button"
                  className={`result-row ${guide.id === selected?.id ? "is-selected" : ""}`}
                  aria-current={guide.id === selected?.id ? "true" : undefined}
                  onClick={(event) => openGuide(guide.id, event.currentTarget)}
                  onKeyDown={(event) => handleResultKey(event, index)}
                >
                  <span className="result-heading">
                    <strong>{guide.title}</strong>
                    <span>{formatUpdated(guide.updatedAt)}</span>
                  </span>
                  <span className="result-summary">{guide.summary}</span>
                  <span className="result-footer">
                    <span>{guide.category}</span>
                    <Status value={guide.status} />
                  </span>
                </button>
              </li>
            ))
          ) : (
            <li className="empty-results">
              <Icon name={pendingWorkspace === null ? "search" : "folder"} />
              <h2>
                {pendingWorkspace !== null
                  ? "Create This Workspace"
                  : workspace === null
                    ? "Open a Workspace"
                    : guides.length === 0 && query.trim() === ""
                      ? "No Guides Yet"
                      : "No Matching Guides"}
              </h2>
              <p>
                {pendingWorkspace !== null
                  ? pendingWorkspace.writable
                    ? "The selected folder is empty. Give the Workspace a name to initialize it."
                    : "The selected folder is read-only. Choose a writable empty folder."
                  : workspace === null
                    ? "Choose a Rehow Workspace to load its Guides securely."
                    : guides.length === 0 && query.trim() === ""
                      ? "This Workspace is ready for its first Guide."
                      : "Try fewer words or search for a command, path, or error message."}
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
                      {initializingWorkspace ? "Creating…" : "Create Workspace"}
                    </button>
                  </div>
                </form>
              ) : workspace === null ? (
                <button
                  className="button"
                  type="button"
                  onClick={openWorkspace}
                >
                  Open Workspace
                </button>
              ) : guides.length === 0 && query.trim() === "" ? null : (
                <button
                  className="button"
                  type="button"
                  onClick={() => setQuery("")}
                >
                  Clear Search
                </button>
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
                            updatingGuideStatus ||
                            removingGuide
                          }
                        >
                          {verifyingGuide ? "Confirming…" : "Mark Tested"}
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
                    onChange={(event) => setDraftBody(event.target.value)}
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

      <footer className="status-bar">
        <span>
          <span className="sync-indicator" aria-hidden="true" />
          {resultCountLabel(guides.length)} loaded
        </span>
        <span className="status-path" translate="no">
          {workspace?.name ?? "No Workspace"}
        </span>
        <span>{workspace ? gitFactLabel(workspace.facts) : "Read-only"}</span>
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
