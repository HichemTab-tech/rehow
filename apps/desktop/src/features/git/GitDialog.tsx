import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  CloudDownload,
  CloudUpload,
  GitBranch,
  GitCommitHorizontal,
  Link2,
  Loader2,
  RefreshCw,
  UserRound,
} from "lucide-react";
import { useEffect, useState } from "react";
import { gitSyncPresentation } from "@/api/payloads";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
} from "@/components/ui/dialog";
import { Field, Input } from "@/components/ui/input";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import { closeDialog, refreshGitStatus, useApp } from "@/store/app";
import {
  addRemote,
  applyPull,
  applyPush,
  cancelCommit,
  clearGitError,
  confirmCommit,
  dismissInspection,
  initializeGit,
  inspectSync,
  prepareCommit,
  recoverGit,
  setAuthor,
  useGit,
} from "./gitStore";

export function GitDialog() {
  const open = useApp((state) => state.dialog === "git");
  const status = useApp((state) => state.gitStatus);
  const error = useGit((state) => state.error);

  useEffect(() => {
    if (open) {
      clearGitError();
      void refreshGitStatus();
    } else {
      void cancelCommit();
      void dismissInspection();
    }
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={(next) => !next && closeDialog()}>
      <DialogContent className="w-[min(92vw,540px)]">
        <DialogHeader
          title="Source control"
          description="Git keeps a history of your Guides and syncs them between machines. It's optional."
        />
        <DialogBody className="space-y-4 pb-5">
          {error ? (
            <div
              role="alert"
              className="flex gap-2 rounded-lg border border-danger/30 bg-danger-soft px-3 py-2 text-sm text-danger"
            >
              <AlertTriangle className="mt-0.5 size-4 shrink-0" />
              <span>{error}</span>
            </div>
          ) : null}
          {status === null ? (
            <p className="flex items-center gap-2 text-sm text-muted">
              <Loader2 className="size-4 animate-spin" /> Checking Git…
            </p>
          ) : status.kind === "unavailable" ? (
            <Empty
              title="Git isn't installed"
              text="Install Git with your package manager, then reopen this panel. Your Guides work fine without it."
            />
          ) : status.kind === "notRepository" ? (
            <NotRepository />
          ) : status.kind === "recoveryRequired" ? (
            <Recovery />
          ) : status.kind === "unsafe" || status.kind === "failed" ? (
            <Empty
              title="Rehow can't use Git in this folder"
              text={
                status.failure ??
                "The repository has settings Rehow doesn't trust (for example, it's owned by another user). Your Guides still work; manage Git from a terminal."
              }
            />
          ) : (
            <Ready />
          )}
        </DialogBody>
      </DialogContent>
    </Dialog>
  );
}

function Empty({ title, text }: { title: string; text: string }) {
  return (
    <div className="rounded-lg border border-border bg-bg px-4 py-4">
      <p className="text-sm font-medium">{title}</p>
      <p className="mt-1 text-sm text-muted">{text}</p>
    </div>
  );
}

function NotRepository() {
  const busy = useGit((state) => state.busy);
  return (
    <div className="rounded-lg border border-border bg-bg px-4 py-4">
      <p className="text-sm font-medium">This Workspace isn't using Git yet</p>
      <p className="mt-1 text-sm text-muted">
        Set it up to keep a history of every change and sync with GitHub, GitLab
        or your own server.
      </p>
      <Button
        className="mt-3"
        variant="primary"
        onClick={() => void initializeGit()}
        disabled={busy !== null}
      >
        {busy === "init" ? "Setting up…" : "Set up Git"}
      </Button>
    </div>
  );
}

function Recovery() {
  const busy = useGit((state) => state.busy);
  return (
    <div className="rounded-lg border border-warning-border bg-warning-soft px-4 py-4">
      <p className="text-sm font-medium">A Git operation was interrupted</p>
      <p className="mt-1 text-sm text-muted">
        Rehow saved enough to finish it safely. Nothing has been lost.
      </p>
      <Button
        className="mt-3"
        onClick={() => void recoverGit()}
        disabled={busy !== null}
      >
        {busy === "recover" ? "Finishing…" : "Finish it now"}
      </Button>
    </div>
  );
}

function Ready() {
  const status = useApp((state) => state.gitStatus);
  if (status === null || status.kind !== "ready") return null;
  const changes = status.changed + status.untracked;
  return (
    <>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 rounded-lg border border-border bg-bg px-3 py-2 text-sm">
        <span className="flex items-center gap-1.5 font-medium">
          <GitBranch className="size-4 text-faint" />
          {status.detached ? "Detached HEAD" : (status.branch ?? "main")}
        </span>
        <span className="text-muted">
          {changes === 0
            ? "No uncommitted changes"
            : plural(changes, "uncommitted change")}
        </span>
        {status.divergenceKnown && status.upstream ? (
          <span className="flex items-center gap-2 text-muted">
            <span className="flex items-center" title="Commits to push">
              <ArrowUp className="size-3.5" />
              {status.ahead}
            </span>
            <span className="flex items-center" title="Commits to pull">
              <ArrowDown className="size-3.5" />
              {status.behind}
            </span>
          </span>
        ) : null}
        <button
          type="button"
          aria-label="Refresh Git status"
          onClick={() => void refreshGitStatus()}
          className="ml-auto inline-flex size-6 items-center justify-center rounded text-faint hover:bg-hover hover:text-text"
        >
          <RefreshCw className="size-3.5" />
        </button>
      </div>
      {status.conflicts > 0 ? (
        <Empty
          title={`${plural(status.conflicts, "file has", "files have")} merge conflicts`}
          text="Rehow never resolves conflicts automatically. Resolve them in a terminal or your editor, then come back."
        />
      ) : null}
      {status.sandbox === "unsandboxed" ? (
        <Empty
          title="Install bubblewrap to commit and sync"
          text="Rehow runs Git inside a sandbox so a repository can't run code on your machine. Bubblewrap isn't available here, so Rehow can show status but won't commit, pull or push. Install the “bubblewrap” package, or use Git from a terminal."
        />
      ) : status.identity === "missing" ? (
        <AuthorSetup canImport={status.globalIdentity === "available"} />
      ) : status.identity === "invalid" ? (
        <Empty
          title="Git author settings look broken"
          text="Fix user.name and user.email in this repository's Git config, then refresh."
        />
      ) : (
        <CommitSection changes={changes} />
      )}
      {status.sandbox === "unsandboxed" ? null : status.remoteCount === 0 ? (
        <RemoteSetup />
      ) : (
        <SyncSection hasUpstream={status.upstream !== null} />
      )}
    </>
  );
}

function Card({
  icon,
  title,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border border-border">
      <h3 className="flex items-center gap-2 border-b border-border px-4 py-2.5 text-sm font-semibold [&_svg]:size-4 [&_svg]:text-faint">
        {icon}
        {title}
      </h3>
      <div className="space-y-3 px-4 py-3">{children}</div>
    </section>
  );
}

function AuthorSetup({ canImport }: { canImport: boolean }) {
  const busy = useGit((state) => state.busy);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  return (
    <Card icon={<UserRound />} title="Who are commits from?">
      <p className="text-sm text-muted">
        Git needs a name and email before it can save history.
      </p>
      {canImport ? (
        <Button
          onClick={() => void setAuthor("global")}
          disabled={busy !== null}
        >
          Use my usual Git name and email
        </Button>
      ) : null}
      <form
        className="grid gap-3 sm:grid-cols-2"
        onSubmit={(event) => {
          event.preventDefault();
          void setAuthor({ name, email });
        }}
      >
        <Field label="Name" htmlFor="git-author-name">
          <Input
            id="git-author-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            autoComplete="name"
          />
        </Field>
        <Field label="Email" htmlFor="git-author-email">
          <Input
            id="git-author-email"
            type="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            autoComplete="email"
          />
        </Field>
        <div className="sm:col-span-2">
          <Button
            type="submit"
            disabled={
              busy !== null || name.trim() === "" || !email.includes("@")
            }
          >
            Save author
          </Button>
        </div>
      </form>
    </Card>
  );
}

function CommitSection({ changes }: { changes: number }) {
  const busy = useGit((state) => state.busy);
  const plan = useGit((state) => state.commitPlan);
  const [message, setMessage] = useState("");

  if (plan) {
    const managed = plan.created + plan.updated + plan.deleted;
    return (
      <Card icon={<GitCommitHorizontal />} title="Review commit">
        <p className="text-sm">
          <span className="font-medium">“{message.trim()}”</span>
          <span className="text-muted"> on {plan.branch}</span>
        </p>
        <ul className="grid grid-cols-3 gap-2 text-center text-xs">
          {(
            [
              ["Added", plan.created],
              ["Changed", plan.updated],
              ["Removed", plan.deleted],
            ] as const
          ).map(([label, count]) => (
            <li key={label} className="rounded-md bg-bg px-2 py-2">
              <span className="block text-lg font-semibold tabular-nums">
                {count}
              </span>
              <span className="text-muted">{label}</span>
            </li>
          ))}
        </ul>
        {plan.unrelatedChanges > 0 ? (
          <p className="text-xs text-muted">
            {plural(plan.unrelatedChanges, "other file")} outside your Guides
            will be left uncommitted.
          </p>
        ) : null}
        <div className="flex justify-end gap-2">
          <Button
            variant="ghost"
            onClick={() => void cancelCommit()}
            disabled={busy !== null}
          >
            Back
          </Button>
          <Button
            variant="primary"
            onClick={() =>
              void confirmCommit().then((ok) => ok && setMessage(""))
            }
            disabled={busy !== null || managed === 0}
          >
            {busy === "commit" ? "Committing…" : "Commit"}
          </Button>
        </div>
      </Card>
    );
  }

  return (
    <Card icon={<GitCommitHorizontal />} title="Commit changes">
      {changes === 0 ? (
        <p className="flex items-center gap-2 text-sm text-muted">
          <CheckCircle2 className="size-4 text-accent" /> Everything is
          committed.
        </p>
      ) : (
        <form
          className="flex gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void prepareCommit(message);
          }}
        >
          <Input
            aria-label="Commit message"
            value={message}
            onChange={(event) => setMessage(event.target.value)}
            placeholder="Describe what changed, e.g. “Add WireGuard guide”"
            maxLength={200}
          />
          <Button
            type="submit"
            disabled={busy !== null || message.trim() === ""}
          >
            {busy === "commit" ? "Checking…" : "Review"}
          </Button>
        </form>
      )}
    </Card>
  );
}

function RemoteSetup() {
  const busy = useGit((state) => state.busy);
  const [url, setUrl] = useState("");
  return (
    <Card icon={<Link2 />} title="Sync with a remote">
      <p className="text-sm text-muted">
        Connect a repository (GitHub, GitLab, your own server) to back up and
        sync your Guides.
      </p>
      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          void addRemote("origin", url).then((ok) => ok && setUrl(""));
        }}
      >
        <Input
          aria-label="Repository address"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
          placeholder="git@github.com:you/guides.git"
          spellCheck={false}
        />
        <Button type="submit" disabled={busy !== null || url.trim() === ""}>
          Connect
        </Button>
      </form>
      <p className="text-xs text-faint">
        Rehow uses your existing SSH keys or Git credential helper. It never
        asks for or stores passwords.
      </p>
    </Card>
  );
}

function SyncSection({ hasUpstream }: { hasUpstream: boolean }) {
  const busy = useGit((state) => state.busy);
  const inspection = useGit((state) => state.inspection);
  const view = inspection ? gitSyncPresentation(inspection) : null;

  if (!hasUpstream) {
    return (
      <Card icon={<CloudUpload />} title="Sync">
        <p className="text-sm text-muted">
          This branch isn't linked to a remote branch yet. Push it once from a
          terminal with{" "}
          <code className="rounded bg-code px-1 font-mono text-xs">
            git push -u origin main
          </code>
          , then sync here.
        </p>
      </Card>
    );
  }

  return (
    <Card icon={<CloudDownload />} title="Sync">
      {inspection && view ? (
        <>
          <div>
            <p className="text-sm font-medium">
              {friendlyTitle(inspection.ancestry, inspection.localChanges)}
            </p>
            <p className="mt-0.5 text-sm text-muted">
              {friendlyDetail(inspection.ancestry, inspection.localChanges)}
            </p>
          </div>
          <div className="flex justify-end gap-2">
            <Button
              variant="ghost"
              onClick={() => void dismissInspection()}
              disabled={busy !== null && busy !== "sync"}
            >
              Close
            </Button>
            {view.canApply ? (
              <Button
                variant="primary"
                onClick={() => void applyPull()}
                disabled={busy !== null}
              >
                <CloudDownload />{" "}
                {busy === "sync" ? "Pulling…" : "Pull changes"}
              </Button>
            ) : null}
            {view.canPush ? (
              <Button
                variant="primary"
                onClick={() => void applyPush()}
                disabled={busy !== null}
              >
                <CloudUpload /> {busy === "push" ? "Pushing…" : "Push commits"}
              </Button>
            ) : null}
          </div>
        </>
      ) : (
        <div className="flex items-center justify-between gap-3">
          <p className="text-sm text-muted">
            Check the remote for new commits, or push yours.
          </p>
          <Button onClick={() => void inspectSync()} disabled={busy !== null}>
            <RefreshCw className={cn(busy === "sync" && "animate-spin")} />
            {busy === "sync" ? "Checking…" : "Check now"}
          </Button>
        </div>
      )}
    </Card>
  );
}

function friendlyTitle(ancestry: string, localChanges: boolean) {
  switch (ancestry) {
    case "upToDate":
      return "You're up to date";
    case "localAhead":
      return "You have commits to push";
    case "diverged":
      return "Your copy and the remote have both changed";
    case "historyLimit":
      return "Couldn't compare histories";
    default:
      return localChanges
        ? "Commit first, then pull"
        : "New changes are available";
  }
}

function friendlyDetail(ancestry: string, localChanges: boolean) {
  switch (ancestry) {
    case "upToDate":
      return "Nothing to pull or push.";
    case "localAhead":
      return "Push them to update the remote. Uncommitted edits are not included.";
    case "diverged":
      return "Rehow won't merge automatically. Merge or rebase in a terminal, then check again.";
    case "historyLimit":
      return "The histories are too far apart to check safely. Review them in a terminal.";
    default:
      return localChanges
        ? "Pulling now could overwrite your uncommitted edits, so commit them first."
        : "Pull to update your Guides with the latest from the remote.";
  }
}
