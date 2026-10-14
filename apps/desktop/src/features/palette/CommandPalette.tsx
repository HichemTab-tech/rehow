import * as DialogPrimitive from "@radix-ui/react-dialog";
import { Command } from "cmdk";
import {
  BadgeCheck,
  FilePlus2,
  FileText,
  FolderOpen,
  FolderPlus,
  GitBranch,
  Keyboard,
  Moon,
  PanelLeft,
  Pencil,
  RefreshCw,
  Settings,
  Sun,
  TriangleAlert,
} from "lucide-react";
import { useMemo, useState } from "react";
import { Kbd } from "@/components/ui/input";
import { StatusDot } from "@/components/ui/status";
import { useCategoryDialog } from "@/features/categories/CategoryDialog";
import { formatRelative, modKey } from "@/lib/format";
import { setTheme, useTheme } from "@/lib/theme";
import {
  closeDialog,
  markTested,
  openDialog,
  openWorkspace,
  refreshWorkspace,
  selectGuide,
  startEditing,
  toggleSidebar,
  useApp,
} from "@/store/app";

type Action = {
  id: string;
  label: string;
  icon: React.ReactNode;
  shortcut?: string[];
  run: () => void;
  when?: boolean;
  keywords?: string[];
};

export function CommandPalette() {
  const open = useApp((state) => state.dialog === "palette");
  const workspace = useApp((state) => state.workspace);
  const guide = useApp((state) => state.guide);
  const selected = useApp((state) =>
    state.workspace?.guides.find((item) => item.id === state.selectedId),
  );
  const theme = useTheme((state) => state.resolved);
  const openCategoryDialog = useCategoryDialog((state) => state.open);
  const [search, setSearch] = useState("");

  const guides = useMemo(() => {
    const all = workspace?.guides ?? [];
    return search.trim() === ""
      ? [...all]
          .filter((item) => item.status !== "Archived")
          .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
          .slice(0, 6)
      : all;
  }, [workspace, search]);

  const hasWorkspace = workspace !== null;
  const actions: Action[] = [
    {
      id: "new-guide",
      label: "New Guide",
      icon: <FilePlus2 />,
      shortcut: [modKey, "N"],
      run: () => openDialog("newGuide"),
      when: hasWorkspace,
      keywords: ["create", "add", "write"],
    },
    {
      id: "edit",
      label: `Edit “${selected?.title ?? ""}”`,
      icon: <Pencil />,
      shortcut: ["E"],
      run: () => {
        closeDialog();
        startEditing();
      },
      when: guide !== null,
    },
    {
      id: "tested",
      label: "Mark current Guide as tested",
      icon: <BadgeCheck />,
      run: () => {
        closeDialog();
        void markTested();
      },
      when: selected?.status === "Draft" || selected?.status === "Stale",
      keywords: ["verify", "confirm", "works"],
    },
    {
      id: "new-category",
      label: "New Category",
      icon: <FolderPlus />,
      run: () => {
        closeDialog();
        openCategoryDialog(null);
      },
      when: hasWorkspace,
      keywords: ["folder", "group"],
    },
    {
      id: "git",
      label: "Source control: commit and sync",
      icon: <GitBranch />,
      run: () => openDialog("git"),
      when: hasWorkspace,
      keywords: ["git", "push", "pull", "commit", "sync"],
    },
    {
      id: "sidebar",
      label: "Toggle sidebar",
      icon: <PanelLeft />,
      shortcut: [modKey, "B"],
      run: () => {
        closeDialog();
        toggleSidebar();
      },
      when: hasWorkspace,
    },
    {
      id: "reload",
      label: "Reload Workspace from disk",
      icon: <RefreshCw />,
      shortcut: [modKey, "R"],
      run: () => {
        closeDialog();
        void refreshWorkspace();
      },
      when: hasWorkspace,
      keywords: ["refresh"],
    },
    {
      id: "issues",
      label: "Show files that couldn't be read",
      icon: <TriangleAlert />,
      run: () => openDialog("issues"),
      when: (workspace?.issueCount ?? 0) > 0,
    },
    {
      id: "open",
      label: "Open another Workspace…",
      icon: <FolderOpen />,
      run: () => {
        closeDialog();
        void openWorkspace("open");
      },
      keywords: ["switch", "folder"],
    },
    {
      id: "theme",
      label:
        theme === "dark" ? "Switch to light theme" : "Switch to dark theme",
      icon: theme === "dark" ? <Sun /> : <Moon />,
      run: () => setTheme(theme === "dark" ? "light" : "dark"),
      keywords: ["appearance", "mode"],
    },
    {
      id: "settings",
      label: "Settings",
      icon: <Settings />,
      shortcut: [modKey, ","],
      run: () => openDialog("settings"),
      when: hasWorkspace,
      keywords: ["preferences", "mcp", "ai", "claude"],
    },
    {
      id: "shortcuts",
      label: "Keyboard shortcuts",
      icon: <Keyboard />,
      shortcut: ["?"],
      run: () => openDialog("shortcuts"),
      when: hasWorkspace,
    },
  ];

  return (
    <DialogPrimitive.Root
      open={open}
      onOpenChange={(next) => {
        if (!next) closeDialog();
        setSearch("");
      }}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-black/25 data-[state=open]:animate-[fade-in_100ms_ease-out] dark:bg-black/45" />
        <DialogPrimitive.Content
          aria-label="Command palette"
          className="fixed top-[14vh] left-1/2 z-50 w-[min(92vw,600px)] -translate-x-1/2 overflow-hidden rounded-xl border border-border bg-raised shadow-dialog outline-none data-[state=open]:animate-[dialog-in_120ms_cubic-bezier(0.2,0.9,0.3,1)]"
        >
          <DialogPrimitive.Title className="sr-only">
            Command palette
          </DialogPrimitive.Title>
          <DialogPrimitive.Description className="sr-only">
            Search Guides or run a command.
          </DialogPrimitive.Description>
          <Command
            loop
            label="Command palette"
            className="flex max-h-[min(480px,70vh)] flex-col"
          >
            <Command.Input
              value={search}
              onValueChange={setSearch}
              placeholder="Search Guides or type a command…"
              className="h-12 shrink-0 border-b border-border bg-transparent px-4 text-[15px] outline-none placeholder:text-faint"
            />
            <Command.List className="min-h-0 overflow-y-auto overscroll-contain p-1.5 [&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-2xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:text-faint [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-[0.06em]">
              <Command.Empty className="px-4 py-8 text-center text-sm text-muted">
                No Guides or commands match.
              </Command.Empty>
              {guides.length > 0 ? (
                <Command.Group
                  heading={search.trim() === "" ? "Recent Guides" : "Guides"}
                >
                  {guides.map((item) => (
                    <Command.Item
                      key={item.id}
                      value={`guide:${item.id}`}
                      keywords={[
                        item.title,
                        item.summary,
                        item.category,
                        ...item.tags,
                        ...item.aliases,
                      ]}
                      onSelect={() => {
                        closeDialog();
                        useApp.setState({
                          query: "",
                          hits: null,
                          categoryFilter: null,
                          tagFilter: null,
                          scope: item.status === "Archived" ? "all" : "active",
                        });
                        void selectGuide(item.id, { open: true });
                      }}
                      className="group flex h-10 cursor-default items-center gap-3 rounded-lg px-2.5 text-sm data-[selected=true]:bg-hover"
                    >
                      <FileText className="size-4 shrink-0 text-faint" />
                      <span className="min-w-0 flex-1 truncate">
                        {item.title}
                      </span>
                      <span className="max-w-32 truncate text-xs text-faint">
                        {item.category}
                      </span>
                      <StatusDot status={item.status} />
                      <span className="hidden w-16 text-right text-2xs text-faint tabular-nums group-data-[selected=true]:inline">
                        {formatRelative(item.updatedAt)}
                      </span>
                    </Command.Item>
                  ))}
                </Command.Group>
              ) : null}
              <Command.Group heading="Commands">
                {actions
                  .filter((action) => action.when !== false)
                  .map((action) => (
                    <Command.Item
                      key={action.id}
                      value={`action:${action.id}`}
                      keywords={[action.label, ...(action.keywords ?? [])]}
                      onSelect={action.run}
                      className="flex h-9 cursor-default items-center gap-3 rounded-lg px-2.5 text-sm data-[selected=true]:bg-hover [&>svg]:size-4 [&>svg]:shrink-0 [&>svg]:text-faint"
                    >
                      {action.icon}
                      <span className="min-w-0 flex-1 truncate">
                        {action.label}
                      </span>
                      {action.shortcut ? (
                        <span className="flex gap-1">
                          {action.shortcut.map((key) => (
                            <Kbd key={key}>{key}</Kbd>
                          ))}
                        </span>
                      ) : null}
                    </Command.Item>
                  ))}
              </Command.Group>
            </Command.List>
            <div className="flex h-8 shrink-0 items-center gap-3 border-t border-border bg-sidebar/60 px-3 text-2xs text-faint">
              <span className="flex items-center gap-1">
                <Kbd>↑</Kbd>
                <Kbd>↓</Kbd> navigate
              </span>
              <span className="flex items-center gap-1">
                <Kbd>↵</Kbd> open
              </span>
              <span className="flex items-center gap-1">
                <Kbd>esc</Kbd> close
              </span>
            </div>
          </Command>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
