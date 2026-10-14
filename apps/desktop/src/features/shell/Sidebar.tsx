import {
  Archive,
  ArchiveRestore,
  ArrowDown,
  ArrowUp,
  CircleAlert,
  CircleCheck,
  CircleDashed,
  Folder,
  Hash,
  Library,
  MoreHorizontal,
  Pencil,
  Plus,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import type { SearchScope } from "@/api/payloads";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { Tooltip } from "@/components/ui/tooltip";
import { useCategoryDialog } from "@/features/categories/CategoryDialog";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  deleteCategory,
  filterByCategory,
  filterByTag,
  moveCategory,
  openDialog,
  setCategoryArchived,
  setScope,
  toggleSidebar,
  useApp,
} from "@/store/app";

function NavItem({
  icon,
  label,
  count,
  active,
  onClick,
  muted,
  tone,
}: {
  icon: ReactNode;
  label: string;
  count?: number;
  active: boolean;
  onClick: () => void;
  muted?: boolean;
  tone?: "warning";
}) {
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      onClick={onClick}
      className={cn(
        "group flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-sm [&_svg]:size-4 [&_svg]:shrink-0",
        active
          ? "bg-active font-medium text-text [&_svg]:text-accent-text"
          : "text-muted hover:bg-hover hover:text-text [&_svg]:text-faint",
        muted && !active && "opacity-70",
      )}
    >
      {icon}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {count !== undefined && count > 0 ? (
        <span
          className={cn(
            "text-xs tabular-nums",
            tone === "warning" ? "font-medium text-warning" : "text-faint",
          )}
        >
          {count}
        </span>
      ) : null}
    </button>
  );
}

function SectionHeader({
  children,
  action,
}: {
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex h-7 items-center justify-between pr-1 pl-2">
      <h2 className="text-2xs font-semibold text-faint uppercase tracking-[0.06em]">
        {children}
      </h2>
      {action}
    </div>
  );
}

export function Sidebar({ overlay = false }: { overlay?: boolean }) {
  const workspace = useApp((state) => state.workspace);
  const scope = useApp((state) => state.scope);
  const categoryFilter = useApp((state) => state.categoryFilter);
  const tagFilter = useApp((state) => state.tagFilter);
  const openCategoryDialog = useCategoryDialog((state) => state.open);
  const [showAllTags, setShowAllTags] = useState(false);

  const counts = useMemo(() => {
    const guides = workspace?.guides ?? [];
    const result = { active: 0, Tested: 0, Draft: 0, Stale: 0, Archived: 0 };
    for (const guide of guides) {
      result[guide.status]++;
      if (guide.status !== "Archived") result.active++;
    }
    return result;
  }, [workspace]);

  const tags = useMemo(() => {
    const tally = new Map<string, number>();
    for (const guide of workspace?.guides ?? []) {
      if (guide.status === "Archived") continue;
      for (const tag of guide.tags) tally.set(tag, (tally.get(tag) ?? 0) + 1);
    }
    return [...tally.entries()].sort(
      (a, b) => b[1] - a[1] || a[0].localeCompare(b[0]),
    );
  }, [workspace]);

  if (workspace === null) return null;
  const activeCategories = workspace.categories.filter(
    (category) => category.status === "active",
  );
  const archivedCategories = workspace.categories.filter(
    (category) => category.status === "archived",
  );
  const noFilter = categoryFilter === null && tagFilter === null;
  const showScope = (target: SearchScope) => () => {
    filterByCategory(null);
    setScope(target);
    if (overlay) toggleSidebar();
  };
  const visibleTags = showAllTags ? tags : tags.slice(0, 12);

  const content = (
    <nav
      aria-label="Library"
      className={cn(
        "flex h-full w-[232px] shrink-0 flex-col border-r border-border bg-sidebar",
        overlay && "shadow-dialog",
      )}
    >
      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-2 py-3">
        <div className="space-y-px">
          <NavItem
            icon={<Library />}
            label="All Guides"
            count={counts.active}
            active={noFilter && scope === "active"}
            onClick={showScope("active")}
          />
          <NavItem
            icon={<CircleAlert />}
            label="Needs retest"
            count={counts.Stale}
            tone="warning"
            active={noFilter && scope === "stale"}
            onClick={showScope("stale")}
          />
          <NavItem
            icon={<CircleDashed />}
            label="Drafts"
            count={counts.Draft}
            active={noFilter && scope === "draft"}
            onClick={showScope("draft")}
          />
          <NavItem
            icon={<CircleCheck />}
            label="Tested"
            count={counts.Tested}
            active={noFilter && scope === "tested"}
            onClick={showScope("tested")}
          />
          <NavItem
            icon={<Archive />}
            label="Archive"
            count={counts.Archived}
            active={noFilter && scope === "archived"}
            onClick={showScope("archived")}
          />
        </div>

        <section aria-labelledby="sidebar-categories">
          <SectionHeader
            action={
              <Tooltip label="New Category">
                <button
                  type="button"
                  aria-label="New Category"
                  onClick={() => openCategoryDialog(null)}
                  disabled={!workspace.facts.writable}
                  className="inline-flex size-6 items-center justify-center rounded-md text-faint hover:bg-hover hover:text-text disabled:opacity-40"
                >
                  <Plus className="size-3.5" />
                </button>
              </Tooltip>
            }
          >
            <span id="sidebar-categories">Categories</span>
          </SectionHeader>
          {activeCategories.length === 0 ? (
            <button
              type="button"
              onClick={() => openCategoryDialog(null)}
              className="mx-2 mt-1 w-[calc(100%-1rem)] rounded-md border border-dashed border-border-strong px-2 py-2 text-left text-xs text-muted hover:bg-hover"
            >
              Create a Category to start organizing Guides.
            </button>
          ) : (
            <ul className="space-y-px">
              {activeCategories.map((category, index) => (
                <li key={category.id}>
                  <CategoryRow
                    category={category}
                    active={categoryFilter === category.id}
                    first={index === 0}
                    last={index === activeCategories.length - 1}
                    onSelect={() => {
                      filterByCategory(
                        categoryFilter === category.id ? null : category.id,
                      );
                      if (overlay) toggleSidebar();
                    }}
                  />
                </li>
              ))}
              {archivedCategories.map((category) => (
                <li key={category.id}>
                  <CategoryRow
                    category={category}
                    active={categoryFilter === category.id}
                    first
                    last
                    onSelect={() => {
                      filterByCategory(
                        categoryFilter === category.id ? null : category.id,
                      );
                      setScope("all");
                    }}
                  />
                </li>
              ))}
            </ul>
          )}
        </section>

        {tags.length > 0 ? (
          <section aria-labelledby="sidebar-tags">
            <SectionHeader>
              <span id="sidebar-tags">Tags</span>
            </SectionHeader>
            <ul className="flex flex-wrap gap-1 px-2 pt-1">
              {visibleTags.map(([tag, count]) => (
                <li key={tag}>
                  <button
                    type="button"
                    aria-pressed={tagFilter === tag}
                    title={plural(count, "Guide")}
                    onClick={() => filterByTag(tagFilter === tag ? null : tag)}
                    className={cn(
                      "inline-flex h-6 items-center gap-1 rounded-md border px-1.5 text-xs",
                      tagFilter === tag
                        ? "border-accent bg-accent-soft text-accent-text"
                        : "border-border bg-panel text-muted hover:border-border-strong hover:text-text",
                    )}
                  >
                    <Hash className="size-3 opacity-60" />
                    {tag}
                  </button>
                </li>
              ))}
            </ul>
            {tags.length > 12 ? (
              <button
                type="button"
                onClick={() => setShowAllTags(!showAllTags)}
                className="mt-1 ml-2 text-xs text-muted hover:text-text"
              >
                {showAllTags ? "Show fewer" : `Show all ${tags.length}`}
              </button>
            ) : null}
          </section>
        ) : null}
      </div>
      {workspace.issueCount > 0 ? (
        <button
          type="button"
          onClick={() => openDialog("issues")}
          className="m-2 flex items-center gap-2 rounded-md border border-warning-border bg-warning-soft px-2.5 py-2 text-left text-xs text-warning hover:brightness-[0.98]"
        >
          <CircleAlert className="size-4 shrink-0" />
          <span className="flex-1">
            {plural(workspace.issueCount, "file")} couldn't be read
          </span>
          <span className="font-medium underline underline-offset-2">
            Review
          </span>
        </button>
      ) : null}
    </nav>
  );

  if (!overlay) return content;
  return (
    <div className="fixed inset-0 top-11 z-30 flex">
      {content}
      <button
        type="button"
        aria-label="Close sidebar"
        className="flex-1 bg-black/20"
        onClick={toggleSidebar}
      />
    </div>
  );
}

function CategoryRow({
  category,
  active,
  first,
  last,
  onSelect,
}: {
  category: NonNullable<
    ReturnType<typeof useApp.getState>["workspace"]
  >["categories"][number];
  active: boolean;
  first: boolean;
  last: boolean;
  onSelect: () => void;
}) {
  const openCategoryDialog = useCategoryDialog((state) => state.open);
  const archived = category.status === "archived";
  const actions = (
    Item: typeof DropdownMenuItem | typeof ContextMenuItem,
    Separator: () => ReactNode,
  ) => (
    <>
      <Item onSelect={() => openCategoryDialog(category.id)}>
        <Pencil /> Rename…
      </Item>
      {archived ? null : (
        <>
          <Item
            disabled={first}
            onSelect={() => void moveCategory(category.id, -1)}
          >
            <ArrowUp /> Move up
          </Item>
          <Item
            disabled={last}
            onSelect={() => void moveCategory(category.id, 1)}
          >
            <ArrowDown /> Move down
          </Item>
        </>
      )}
      <Separator />
      <Item onSelect={() => void setCategoryArchived(category.id, !archived)}>
        {archived ? <ArchiveRestore /> : <Archive />}
        {archived ? "Restore" : "Archive"}
      </Item>
      <Item destructive onSelect={() => void deleteCategory(category.id)}>
        <Trash2 /> Delete…
      </Item>
    </>
  );

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          className={cn(
            "group relative flex h-7 items-center rounded-md",
            active ? "bg-active" : "hover:bg-hover",
          )}
        >
          <button
            type="button"
            aria-current={active ? "page" : undefined}
            onClick={onSelect}
            className={cn(
              "flex h-full min-w-0 flex-1 items-center gap-2 rounded-md pr-8 pl-2 text-left text-sm",
              active ? "font-medium text-text" : "text-muted hover:text-text",
              archived && "opacity-60",
            )}
          >
            {archived ? (
              <RotateCcw className="size-4 shrink-0 text-faint" />
            ) : (
              <Folder
                className={cn(
                  "size-4 shrink-0",
                  active ? "text-accent-text" : "text-faint",
                )}
              />
            )}
            <span className="min-w-0 flex-1 truncate">{category.name}</span>
            <span className="text-xs text-faint tabular-nums group-hover:opacity-0 group-focus-within:opacity-0">
              {category.guideCount > 0 ? category.guideCount : null}
            </span>
          </button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label={`${category.name} options`}
                className="absolute right-1 inline-flex size-6 items-center justify-center rounded text-faint opacity-0 hover:bg-active hover:text-text focus-visible:opacity-100 group-hover:opacity-100 data-[state=open]:opacity-100"
              >
                <MoreHorizontal className="size-4" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" side="right">
              {actions(DropdownMenuItem, DropdownMenuSeparator)}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        {actions(ContextMenuItem, ContextMenuSeparator)}
      </ContextMenuContent>
    </ContextMenu>
  );
}
