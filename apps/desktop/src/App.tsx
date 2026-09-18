import type { SafeMarkdownDocument } from "@rehow/contracts";
import { useEffect, useMemo, useRef, useState } from "react";
import { MarkdownDocument } from "./MarkdownDocument";

export const APPLICATION_NAME = "Rehow";

type GuideSummary = {
  id: string;
  title: string;
  summary: string;
  category: string;
  status: "Tested" | "Draft" | "Stale" | "Archived";
  updatedAt: string;
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
  categories: Array<{ id: string; name: string; guideCount: number }>;
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

type IconName =
  | "book"
  | "search"
  | "plus"
  | "chevron"
  | "folder"
  | "tag"
  | "archive"
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

export function normalizedWorkspaceName(value: string) {
  const name = value.trim();
  const length = Array.from(name).length;
  return length >= 1 && length <= 200 ? name : null;
}

export function workspaceInitializationError(error: unknown) {
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
    message: "The Workspace could not be created. Choose the folder again.",
  };
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
  const [pendingWorkspace, setPendingWorkspace] = useState<FolderFacts | null>(
    null,
  );
  const [workspaceName, setWorkspaceName] = useState("");
  const [initializingWorkspace, setInitializingWorkspace] = useState(false);
  const [workspaceSetupError, setWorkspaceSetupError] = useState("");
  const [guideBody, setGuideBody] = useState<GuideBodyPayload | null>(null);
  const [editingGuide, setEditingGuide] = useState(false);
  const [draftBody, setDraftBody] = useState("");
  const [savingGuide, setSavingGuide] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [rankedGuideIds, setRankedGuideIds] = useState<string[] | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const readerRef = useRef<HTMLElement>(null);
  const resultListRef = useRef<HTMLUListElement>(null);
  const lastResultRef = useRef<HTMLButtonElement | null>(null);
  const railToggleRef = useRef<HTMLButtonElement>(null);
  const editButtonRef = useRef<HTMLButtonElement>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const saveErrorRef = useRef<HTMLParagraphElement>(null);
  const workspaceNameRef = useRef<HTMLInputElement>(null);
  const workspaceSetupErrorRef = useRef<HTMLParagraphElement>(null);
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
        if (editingGuide) return;
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
  }, [editingGuide, railOpen, readerOpen]);

  useEffect(() => {
    if (!dirtyGuide) return;
    function warnBeforeUnload(event: BeforeUnloadEvent) {
      event.preventDefault();
      event.returnValue = "";
    }
    window.addEventListener("beforeunload", warnBeforeUnload);
    return () => window.removeEventListener("beforeunload", warnBeforeUnload);
  }, [dirtyGuide]);

  useEffect(() => {
    if (workspace === null || selectedId === "") {
      setGuideBody(null);
      return;
    }
    let active = true;
    setGuideBody(null);
    setEditingGuide(false);
    setSaveError("");
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
    if (savingGuide) return false;
    return !dirtyGuide || window.confirm("Discard your unsaved Guide changes?");
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
      const failure = workspaceInitializationError(error);
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

  function beginEditing() {
    if (guideBody === null || guideBody.id !== selected?.id) return;
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
            className="button button-primary"
            type="button"
            title="Guide editing is enabled in the next phase"
            disabled
          >
            <Icon name="plus" />
            New Guide
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
          <p className="rail-label">Categories</p>
          {(workspace?.categories ?? []).map((category) => (
            <button className="category-item" type="button" key={category.id}>
              <span>{category.name}</span>
              <span className="count">{category.guideCount}</span>
            </button>
          ))}
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
                {workspace?.facts.writable && !editingGuide && (
                  <button
                    ref={editButtonRef}
                    className="button guide-edit-button"
                    type="button"
                    onClick={beginEditing}
                    disabled={guideBody?.id !== selected.id}
                  >
                    Edit Guide
                  </button>
                )}
              </div>
              <p className="guide-summary">{selected.summary}</p>
              <dl className="guide-meta">
                <div>
                  <dt>Updated</dt>
                  <dd>{formatLongDate(selected.updatedAt)}</dd>
                </div>
                <div>
                  <dt>Mode</dt>
                  <dd>{editingGuide ? "Editing" : "Reader"}</dd>
                </div>
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
