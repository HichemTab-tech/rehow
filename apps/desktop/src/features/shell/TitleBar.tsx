import {
  BookOpen,
  ChevronDown,
  FolderOpen,
  FolderPlus,
  GitBranch,
  PanelLeft,
  Plus,
  RefreshCw,
  Search,
  Settings,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { Tooltip } from "@/components/ui/tooltip";
import { modKey } from "@/lib/format";
import {
  openDialog,
  openWorkspace,
  refreshWorkspace,
  toggleSidebar,
  useApp,
} from "@/store/app";

export function TitleBar({ compact }: { compact: boolean }) {
  const name = useApp((state) => state.workspace?.name ?? "");
  const writable = useApp((state) => state.workspace?.facts.writable ?? true);

  return (
    <header className="flex h-11 shrink-0 items-center gap-1 border-b border-border bg-sidebar px-2">
      <Tooltip label="Toggle sidebar" shortcut={`${modKey}+B`}>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Toggle sidebar"
          onClick={toggleSidebar}
        >
          <PanelLeft />
        </Button>
      </Tooltip>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="flex h-7 min-w-0 items-center gap-2 rounded-md px-2 text-sm font-semibold hover:bg-hover data-[state=open]:bg-hover"
          >
            <span className="flex size-5 items-center justify-center rounded-[5px] bg-accent text-accent-fg">
              <BookOpen className="size-3" strokeWidth={2.5} />
            </span>
            <span className="truncate">{name}</span>
            {writable ? null : (
              <span className="rounded border border-border px-1 text-2xs font-medium text-muted">
                Read-only
              </span>
            )}
            <ChevronDown className="size-3.5 text-faint" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-60">
          <DropdownMenuLabel>Workspace</DropdownMenuLabel>
          <DropdownMenuItem onSelect={() => void openWorkspace("open")}>
            <FolderOpen /> Open another folder…
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void openWorkspace("create")}>
            <FolderPlus /> New Workspace…
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void openWorkspace("clone")}>
            <GitBranch /> Clone from Git…
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            shortcut={`${modKey}+R`}
            onSelect={() => void refreshWorkspace()}
          >
            <RefreshCw /> Reload from disk
          </DropdownMenuItem>
          <DropdownMenuItem
            shortcut={`${modKey}+,`}
            onSelect={() => openDialog("settings")}
          >
            <Settings /> Settings
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <div className="flex flex-1 justify-center px-2">
        <button
          type="button"
          onClick={() => openDialog("palette")}
          className="flex h-7 w-full max-w-[420px] items-center gap-2 rounded-md border border-border bg-panel px-2.5 text-sm text-faint shadow-[0_1px_1px_rgb(0_0_0/0.03)] hover:border-border-strong hover:text-muted"
        >
          <Search className="size-3.5" />
          <span className="flex-1 truncate text-left">
            {compact ? "Search…" : "Search Guides or run a command…"}
          </span>
          <Kbd>{modKey}</Kbd>
          <Kbd>K</Kbd>
        </button>
      </div>

      <Tooltip label="New Guide" shortcut={`${modKey}+N`}>
        <Button
          variant="primary"
          size="sm"
          onClick={() => openDialog("newGuide")}
          disabled={!writable}
        >
          <Plus /> {compact ? null : "New Guide"}
        </Button>
      </Tooltip>
      <Tooltip label="Settings">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Settings"
          onClick={() => openDialog("settings")}
        >
          <Settings />
        </Button>
      </Tooltip>
    </header>
  );
}
