import { useEffect, useMemo, useRef, useState } from "react";

export const APPLICATION_NAME = "Rehow";

type GuideSummary = {
  id: string;
  title: string;
  summary: string;
  category: string;
  status: "Tested" | "Draft" | "Stale";
  updatedAt: string;
  matches: string;
};

const DEFAULT_GUIDE: GuideSummary = {
  id: "install-tarball",
  title: "Install a Downloaded Tarball",
  summary:
    "Place a portable application in ~/Programs and expose its executable safely.",
  category: "Linux Setup",
  status: "Tested",
  updatedAt: "2026-09-16T18:42:00Z",
  matches: "tarball tar.gz Programs archive executable install",
};

const GUIDES: GuideSummary[] = [
  DEFAULT_GUIDE,
  {
    id: "desktop-launcher",
    title: "Create a Desktop Launcher",
    summary:
      "Add a user-level .desktop entry and refresh the application database.",
    category: "Desktop",
    status: "Tested",
    updatedAt: "2026-09-11T09:15:00Z",
    matches: "desktop launcher application menu icon Exec",
  },
  {
    id: "hyprland-keybinding",
    title: "Change an Omarchy Keybinding",
    summary:
      "Override a Hyprland binding without editing Omarchy-managed files.",
    category: "Omarchy",
    status: "Stale",
    updatedAt: "2026-08-29T21:08:00Z",
    matches: "hyprland keybind omarchy shortcut bind.conf",
  },
  {
    id: "udev-rule",
    title: "Add a udev Rule for a USB Device",
    summary:
      "Identify a device, write a focused rule, reload it, and verify the result.",
    category: "Hardware",
    status: "Draft",
    updatedAt: "2026-08-22T12:20:00Z",
    matches: "udev usb vendor product rule hardware",
  },
];

const CATEGORY_COUNTS = [
  ["Linux Setup", 12],
  ["Desktop", 8],
  ["Omarchy", 7],
  ["Hardware", 4],
] as const;

type IconName =
  | "book"
  | "search"
  | "plus"
  | "copy"
  | "check"
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
    copy: (
      <>
        <rect x="8" y="8" width="11" height="11" rx="1.5" />
        <path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8" />
      </>
    ),
    check: <path d="m5 12 4 4L19 6" />,
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

export function resultCountLabel(count: number) {
  return `${count} ${count === 1 ? "Guide" : "Guides"}`;
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
  const [selectedId, setSelectedId] = useState(
    () => initialParameter("guide") ?? DEFAULT_GUIDE.id,
  );
  const [readerOpen, setReaderOpen] = useState(
    () => initialParameter("guide") !== null,
  );
  const [railOpen, setRailOpen] = useState(
    () => initialParameter("rail") === "open",
  );
  const [announcement, setAnnouncement] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const readerRef = useRef<HTMLElement>(null);
  const resultListRef = useRef<HTMLUListElement>(null);
  const lastResultRef = useRef<HTMLButtonElement | null>(null);
  const railToggleRef = useRef<HTMLButtonElement>(null);
  const visibleGuides = useMemo(() => filterGuides(GUIDES, query), [query]);
  const selected =
    GUIDES.find((guide) => guide.id === selectedId) ??
    visibleGuides[0] ??
    DEFAULT_GUIDE;

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
  }, [railOpen, readerOpen]);

  function openGuide(id: string, trigger?: HTMLButtonElement) {
    if (trigger) lastResultRef.current = trigger;
    setSelectedId(id);
    setReaderOpen(true);
    requestAnimationFrame(() => readerRef.current?.focus());
  }

  function closeReader() {
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
    setSelectedId(guide.id);
    resultListRef.current
      ?.querySelectorAll<HTMLButtonElement>(".result-row")
      .item(next)
      .focus();
  }

  async function copyCommand(command: string) {
    try {
      await navigator.clipboard.writeText(command);
      setAnnouncement("Command copied.");
    } catch {
      setAnnouncement(
        "Couldn’t copy the command. Select it and copy manually.",
      );
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
        <button className="workspace-switcher" type="button">
          <span className="workspace-dot" aria-hidden="true" />
          Personal Notes
          <Icon name="chevron" />
        </button>
        <div className="toolbar-actions">
          <button
            className="button button-primary"
            type="button"
            title="New Guide"
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
          <RailItem icon="book" label="All Guides" count={31} active />
          <RailItem icon="folder" label="Categories" count={6} />
          <RailItem icon="tag" label="Tags" count={18} />
          <RailItem icon="archive" label="Archive" count={3} />
        </nav>
        <div className="category-list">
          <p className="rail-label">Categories</p>
          {CATEGORY_COUNTS.map(([label, count]) => (
            <button className="category-item" type="button" key={label}>
              <span>{label}</span>
              <span className="count">{count}</span>
            </button>
          ))}
        </div>
        <div className="rail-footer">
          <span className="sync-indicator" aria-hidden="true" />
          Workspace ready
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
        </div>
        <ul ref={resultListRef} className="result-list" aria-label="Guides">
          {visibleGuides.length > 0 ? (
            visibleGuides.map((guide, index) => (
              <li key={guide.id}>
                <button
                  type="button"
                  className={`result-row ${guide.id === selected.id ? "is-selected" : ""}`}
                  aria-current={guide.id === selected.id ? "true" : undefined}
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
              <Icon name="search" />
              <h2>No Matching Guides</h2>
              <p>
                Try fewer words or search for a command, path, or error message.
              </p>
              <button
                className="button"
                type="button"
                onClick={() => setQuery("")}
              >
                Clear Search
              </button>
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
        <article className="guide-reader">
          <button className="back-button" type="button" onClick={closeReader}>
            ← Back to Results
          </button>
          <header className="guide-header">
            <div className="breadcrumbs">
              <span>{selected.category}</span>
              <span aria-hidden="true">/</span>
              <span>Installation</span>
            </div>
            <div className="title-line">
              <h1>{selected.title}</h1>
              <Status value={selected.status} />
            </div>
            <p className="guide-summary">{selected.summary}</p>
            <dl className="guide-meta">
              <div>
                <dt>Last Verified</dt>
                <dd>{formatLongDate(selected.updatedAt)}</dd>
              </div>
              <div>
                <dt>Applies To</dt>
                <dd>Arch Linux · Wayland</dd>
              </div>
              <div>
                <dt>Files Affected</dt>
                <dd>
                  <code translate="no">~/Programs</code>
                </dd>
              </div>
            </dl>
          </header>
          <section aria-labelledby="before-heading">
            <h2 id="before-heading">Before You Start</h2>
            <div className="callout callout-warning">
              <strong>Keep the extracted directory.</strong>
              <p>
                The launcher points into this folder. Moving it later will break
                the command path.
              </p>
            </div>
            <ul className="check-list">
              <li>
                Download the Linux tarball from the project’s official release
                page.
              </li>
              <li>Confirm the archive matches your CPU architecture.</li>
            </ul>
          </section>
          <section aria-labelledby="steps-heading">
            <h2 id="steps-heading">Steps</h2>
            <ol className="steps">
              <Step
                title="Create the Programs Directory"
                copy="Keep manually installed applications separate from system packages."
                command="mkdir -p ~/Programs"
                onCopy={copyCommand}
              />
              <Step
                title="Extract the Archive"
                copy="Replace the archive name with the file you downloaded."
                command="tar -xvf ~/Downloads/example-linux-x86_64.tar.gz -C ~/Programs"
                onCopy={copyCommand}
              />
              <Step
                title="Verify the Executable"
                copy="Run the application directly before creating a launcher."
                command="~/Programs/example/example"
                onCopy={copyCommand}
              />
            </ol>
          </section>
          <section aria-labelledby="verify-heading">
            <h2 id="verify-heading">Verification</h2>
            <div className="verification">
              <Icon name="check" />
              <div>
                <strong>The application opens without terminal errors.</strong>
                <p>
                  Close it normally, then continue with the related desktop
                  launcher Guide.
                </p>
              </div>
            </div>
          </section>
          <section aria-labelledby="rollback-heading">
            <h2 id="rollback-heading">Rollback</h2>
            <p>
              Remove the extracted application directory. This does not affect
              packages managed by pacman.
            </p>
            <CommandBlock
              command="rm -r ~/Programs/example"
              onCopy={copyCommand}
            />
          </section>
        </article>
      </main>

      <footer className="status-bar">
        <span>
          <span className="sync-indicator" aria-hidden="true" />
          31 Guides indexed
        </span>
        <span className="status-path" translate="no">
          ~/Documents/Rehow
        </span>
        <span>Git: 2 changes</span>
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

function Step({
  title,
  copy,
  command,
  onCopy,
}: {
  title: string;
  copy: string;
  command: string;
  onCopy: (value: string) => void;
}) {
  return (
    <li>
      <div className="step-copy">
        <h3>{title}</h3>
        <p>{copy}</p>
      </div>
      <CommandBlock command={command} onCopy={onCopy} />
    </li>
  );
}

function CommandBlock({
  command,
  onCopy,
}: {
  command: string;
  onCopy: (command: string) => void;
}) {
  return (
    <div className="command-block">
      <input
        className="command-scroll"
        aria-label="Command, horizontally scrollable"
        name="documented-command"
        type="text"
        autoComplete="off"
        spellCheck={false}
        readOnly
        translate="no"
        value={command}
      />
      <button
        type="button"
        onClick={() => onCopy(command)}
        aria-label="Copy command"
        title="Copy command"
      >
        <Icon name="copy" />
      </button>
    </div>
  );
}
