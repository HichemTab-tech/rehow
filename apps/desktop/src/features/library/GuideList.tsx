import {
  ArrowDownUp,
  FileSearch,
  Folder,
  Hash,
  Paperclip,
  Plus,
  Search,
  X,
} from "lucide-react";
import { memo, useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  type GuideSummary,
  highlightedExcerptSegments,
  type SearchHitPayload,
} from "@/api/payloads";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { StatusDot, statusLabel } from "@/components/ui/status";
import { moveSelection } from "@/features/shell/useAppEffects";
import { formatRelative, plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  clearFilters,
  filterByCategory,
  filterByTag,
  openDialog,
  type SortOrder,
  selectGuide,
  setQuery,
  setScope,
  useApp,
  visibleGuides,
} from "@/store/app";

const SCOPE_LABELS: Record<string, string> = {
  stale: "Needs retest",
  draft: "Drafts",
  tested: "Tested",
  archived: "Archive",
  all: "Everything",
};

const SORT_LABELS: Record<SortOrder, string> = {
  updated: "Recently updated",
  title: "Title",
  status: "Needs attention first",
};

const FIELD_LABELS: Record<SearchHitPayload["excerptField"], string> = {
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

export function GuideList({ className }: { className?: string }) {
  const state = useApp(
    useShallow((app) => ({
      workspace: app.workspace,
      query: app.query,
      scope: app.scope,
      categoryFilter: app.categoryFilter,
      tagFilter: app.tagFilter,
      sort: app.sort,
      hits: app.hits,
      selectedId: app.selectedId,
    })),
  );
  const guides = useMemo(() => visibleGuides(state), [state]);
  const hits = useMemo(
    () => new Map((state.hits ?? []).map((hit) => [hit.guideId, hit])),
    [state.hits],
  );
  const category = state.workspace?.categories.find(
    (item) => item.id === state.categoryFilter,
  );
  const searching = state.query.trim() !== "";

  return (
    <div className={cn("flex min-h-0 flex-col bg-bg", className)}>
      <div className="space-y-2 border-b border-border px-3 pt-3 pb-2">
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-faint" />
          <input
            id="guide-search"
            type="search"
            value={state.query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                moveSelection(1);
              } else if (event.key === "ArrowUp") {
                event.preventDefault();
                moveSelection(-1);
              } else if (event.key === "Enter") {
                event.preventDefault();
                document.getElementById("guide-reader")?.focus();
              } else if (event.key === "Escape" && state.query !== "") {
                event.preventDefault();
                setQuery("");
              }
            }}
            placeholder="Search titles, commands, errors…"
            aria-label="Search Guides"
            autoComplete="off"
            spellCheck={false}
            className="h-8 w-full rounded-md border border-border-strong bg-panel pr-8 pl-8 text-sm outline-none placeholder:text-faint focus-visible:border-ring focus-visible:shadow-[0_0_0_3px_color-mix(in_srgb,var(--ring)_22%,transparent)] focus-visible:outline-none [&::-webkit-search-cancel-button]:hidden"
          />
          {state.query ? (
            <button
              type="button"
              aria-label="Clear search"
              onClick={() => setQuery("")}
              className="absolute top-1/2 right-1.5 inline-flex size-5 -translate-y-1/2 items-center justify-center rounded text-faint hover:bg-hover hover:text-text"
            >
              <X className="size-3.5" />
            </button>
          ) : (
            <Kbd className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2">
              /
            </Kbd>
          )}
        </div>
        <div className="flex min-h-6 items-center gap-1.5">
          {category ? (
            <FilterChip
              onClear={() => filterByCategory(null)}
              icon={<Folder />}
            >
              {category.name}
            </FilterChip>
          ) : null}
          {state.tagFilter ? (
            <FilterChip onClear={() => filterByTag(null)} icon={<Hash />}>
              {state.tagFilter}
            </FilterChip>
          ) : null}
          {state.scope !== "active" ? (
            <FilterChip onClear={() => setScope("active")}>
              {SCOPE_LABELS[state.scope]}
            </FilterChip>
          ) : null}
          <span
            className="flex-1 truncate text-xs text-faint tabular-nums"
            aria-live="polite"
          >
            {plural(guides.length, "Guide")}
          </span>
          {searching ? (
            <span className="text-xs text-faint">Best match</span>
          ) : (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  className="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-xs text-muted hover:bg-hover hover:text-text"
                >
                  <ArrowDownUp className="size-3.5" />
                  {SORT_LABELS[state.sort]}
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>Sort by</DropdownMenuLabel>
                <DropdownMenuRadioGroup
                  value={state.sort}
                  onValueChange={(value) =>
                    useApp.setState({ sort: value as SortOrder })
                  }
                >
                  {(Object.keys(SORT_LABELS) as SortOrder[]).map((sort) => (
                    <DropdownMenuRadioItem key={sort} value={sort}>
                      {SORT_LABELS[sort]}
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>
      </div>

      {guides.length === 0 ? (
        <EmptyList
          searching={searching}
          filtered={
            category !== undefined ||
            state.tagFilter !== null ||
            state.scope !== "active"
          }
        />
      ) : (
        <ul
          id="guide-list"
          aria-label="Guides"
          className="min-h-0 flex-1 overflow-y-auto p-1.5"
        >
          {guides.map((guide) => (
            <GuideRow
              key={guide.id}
              guide={guide}
              hit={hits.get(guide.id)}
              selected={guide.id === state.selectedId}
              showCategory={state.categoryFilter === null}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function FilterChip({
  children,
  icon,
  onClear,
}: {
  children: React.ReactNode;
  icon?: React.ReactNode;
  onClear: () => void;
}) {
  return (
    <span className="inline-flex h-6 max-w-[140px] items-center gap-1 rounded-md border border-accent/40 bg-accent-soft pr-0.5 pl-1.5 text-xs font-medium text-accent-text [&>svg]:size-3 [&>svg]:shrink-0">
      {icon}
      <span className="truncate">{children}</span>
      <button
        type="button"
        aria-label={`Remove filter ${typeof children === "string" ? children : ""}`}
        onClick={onClear}
        className="inline-flex size-5 items-center justify-center rounded hover:bg-accent/15"
      >
        <X className="size-3" />
      </button>
    </span>
  );
}

const GuideRow = memo(function GuideRow({
  guide,
  hit,
  selected,
  showCategory,
}: {
  guide: GuideSummary;
  hit: SearchHitPayload | undefined;
  selected: boolean;
  showCategory: boolean;
}) {
  const showExcerpt =
    hit !== undefined &&
    hit.excerptField !== "title" &&
    hit.excerptField !== "summary";
  return (
    <li className="[contain-intrinsic-size:auto_72px] [content-visibility:auto]">
      <button
        type="button"
        data-guide-row={guide.id}
        aria-current={selected ? "true" : undefined}
        onClick={() => void selectGuide(guide.id, { open: true })}
        onDoubleClick={() => document.getElementById("guide-reader")?.focus()}
        className={cn(
          "group flex w-full flex-col gap-1 rounded-lg px-3 py-2.5 text-left outline-offset-[-2px]",
          selected
            ? "bg-panel shadow-[0_0_0_1px_var(--border-strong),0_1px_2px_rgb(0_0_0/0.04)]"
            : "hover:bg-hover",
        )}
      >
        <span className="flex items-start gap-2">
          <span
            className={cn(
              "min-w-0 flex-1 text-[13.5px] leading-5 font-medium",
              guide.status === "Archived" ? "text-muted" : "text-text",
            )}
          >
            {hit?.excerptField === "title" ? (
              <Highlighted hit={hit} />
            ) : (
              guide.title
            )}
          </span>
          <span
            className="mt-1.5 flex shrink-0 items-center"
            title={statusLabel(guide.status)}
          >
            <StatusDot status={guide.status} />
            <span className="sr-only">{statusLabel(guide.status)}</span>
          </span>
        </span>
        {showExcerpt ? (
          <span className="line-clamp-2 text-xs leading-[17px] text-muted">
            <span className="mr-1 font-medium text-faint">
              {FIELD_LABELS[hit.excerptField]}
            </span>
            <Highlighted hit={hit} />
          </span>
        ) : guide.summary === "" && hit === undefined ? null : (
          <span className="line-clamp-2 text-xs leading-[17px] text-muted">
            {hit?.excerptField === "summary" ? (
              <Highlighted hit={hit} />
            ) : (
              guide.summary
            )}
          </span>
        )}
        <span className="flex min-w-0 items-center gap-1.5 text-2xs text-faint">
          {showCategory ? (
            <span className="max-w-[55%] shrink-0 truncate">
              {guide.category}
            </span>
          ) : null}
          {showCategory && guide.tags.length > 0 ? (
            <span aria-hidden="true">·</span>
          ) : null}
          <span className="min-w-0 truncate">
            {guide.tags
              .slice(0, 3)
              .map((tag) => `#${tag}`)
              .join(" ")}
          </span>
          {guide.attachments.length > 0 ? (
            <span
              className="flex items-center gap-0.5"
              title={plural(guide.attachments.length, "attachment")}
            >
              <Paperclip className="size-3" />
              {guide.attachments.length}
            </span>
          ) : null}
          <span className="ml-auto shrink-0 tabular-nums">
            {formatRelative(guide.updatedAt)}
          </span>
        </span>
      </button>
    </li>
  );
});

function Highlighted({ hit }: { hit: SearchHitPayload }) {
  return (
    <>
      {highlightedExcerptSegments(hit).map((segment) =>
        segment.highlighted ? (
          <mark
            key={`${segment.start}-m`}
            className="rounded-[2px] bg-[var(--mark)] px-px text-text"
          >
            {segment.text}
          </mark>
        ) : (
          <span key={`${segment.start}-t`}>{segment.text}</span>
        ),
      )}
    </>
  );
}

function EmptyList({
  searching,
  filtered,
}: {
  searching: boolean;
  filtered: boolean;
}) {
  const hasGuides = useApp(
    (state) => (state.workspace?.guides.length ?? 0) > 0,
  );
  const query = useApp((state) => state.query);
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-8 pb-16 text-center">
      <div className="flex size-10 items-center justify-center rounded-full bg-hover text-faint">
        <FileSearch className="size-5" />
      </div>
      {!hasGuides ? (
        <>
          <div className="space-y-1">
            <p className="text-sm font-medium">No Guides yet</p>
            <p className="text-xs text-muted">
              Write down how you did something so future you doesn't have to
              figure it out again.
            </p>
          </div>
          <Button
            variant="primary"
            size="sm"
            onClick={() => openDialog("newGuide")}
          >
            <Plus /> New Guide
          </Button>
        </>
      ) : searching ? (
        <>
          <div className="space-y-1">
            <p className="text-sm font-medium">
              Nothing matches “{query.trim()}”
            </p>
            <p className="text-xs text-muted">
              Try fewer words, or search for a command, path or error message.
            </p>
          </div>
          <div className="flex gap-2">
            {filtered ? (
              <Button size="sm" onClick={clearFilters}>
                Clear filters
              </Button>
            ) : null}
            <Button size="sm" onClick={() => setScope("all")}>
              Include archived
            </Button>
          </div>
        </>
      ) : (
        <>
          <p className="text-sm font-medium">No Guides here</p>
          <Button size="sm" onClick={clearFilters}>
            Show all Guides
          </Button>
        </>
      )}
    </div>
  );
}
