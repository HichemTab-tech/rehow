import { ArrowDown, ArrowUp, CircleAlert, Eye, GitBranch } from "lucide-react";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import { openDialog, useApp } from "@/store/app";

export function StatusBar() {
  const workspace = useApp((state) => state.workspace);
  const git = useApp((state) => state.gitStatus);
  if (workspace === null) return null;
  const active = workspace.guides.filter(
    (guide) => guide.status !== "Archived",
  ).length;
  const changes = git ? git.changed + git.untracked : 0;

  let gitLabel: string;
  let gitAttention = false;
  if (git === null) gitLabel = "Checking Git…";
  else if (git.kind === "notRepository") gitLabel = "Not using Git";
  else if (git.kind === "unavailable") gitLabel = "Git not installed";
  else if (git.kind === "recoveryRequired") {
    gitLabel = "Git needs attention";
    gitAttention = true;
  } else if (git.kind === "unsafe" || git.kind === "failed") {
    gitLabel = "Git unavailable";
    gitAttention = git.kind === "unsafe";
  } else if (git.conflicts > 0) {
    gitLabel = `${git.branch ?? "HEAD"} · ${plural(git.conflicts, "conflict")}`;
    gitAttention = true;
  } else {
    gitLabel = git.detached ? "Detached HEAD" : (git.branch ?? "main");
  }

  return (
    <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-border bg-sidebar px-3 text-2xs text-faint">
      <span className="tabular-nums">{plural(active, "Guide")}</span>
      {workspace.watchStatus === "degraded" ? (
        <span
          className="flex items-center gap-1 text-warning"
          title="Rehow can't watch this folder for changes. Use Reload from disk after editing files elsewhere."
        >
          <Eye className="size-3" /> Not watching for file changes
        </span>
      ) : null}
      {workspace.issueCount > 0 ? (
        <button
          type="button"
          onClick={() => openDialog("issues")}
          className="flex items-center gap-1 text-warning hover:underline"
        >
          <CircleAlert className="size-3" />{" "}
          {plural(workspace.issueCount, "issue")}
        </button>
      ) : null}
      <span className="flex-1" />
      <button
        type="button"
        onClick={() => openDialog("git")}
        className={cn(
          "flex h-5 items-center gap-1.5 rounded px-1.5 hover:bg-hover hover:text-text",
          gitAttention && "text-warning",
        )}
        aria-label={`Source control: ${gitLabel}`}
      >
        <GitBranch className="size-3" />
        <span>{gitLabel}</span>
        {git?.kind === "ready" && changes > 0 ? (
          <span className="text-muted">· {plural(changes, "change")}</span>
        ) : null}
        {git?.kind === "ready" && git.divergenceKnown && git.ahead > 0 ? (
          <span className="flex items-center text-muted">
            <ArrowUp className="size-3" />
            {git.ahead}
          </span>
        ) : null}
        {git?.kind === "ready" && git.divergenceKnown && git.behind > 0 ? (
          <span className="flex items-center text-muted">
            <ArrowDown className="size-3" />
            {git.behind}
          </span>
        ) : null}
      </button>
    </footer>
  );
}
