import {
  ArrowLeft,
  BookOpen,
  ChevronRight,
  FileStack,
  FolderOpen,
  FolderPlus,
  GitBranch,
  Loader2,
} from "lucide-react";
import { motion } from "motion/react";
import { useState } from "react";
import { toast } from "sonner";
import { api, toNativeError } from "@/api/client";
import { type FolderFacts, normalizedCloneUrl } from "@/api/payloads";
import { BlurText } from "@/components/BlurText";
import { Button } from "@/components/ui/button";
import { Field, Input } from "@/components/ui/input";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  adoptWorkspace,
  cancelSetup,
  createWorkspace,
  openWorkspace,
  useApp,
} from "@/store/app";

export function Welcome() {
  const setup = useApp((state) => state.setup);
  const busy = useApp((state) => state.busy);

  return (
    <div className="flex h-full overflow-y-auto bg-bg">
      <div className="m-auto w-full max-w-[520px] px-6 py-12">
        {setup.kind === "none" ? (
          <Start busy={busy !== null} />
        ) : setup.kind === "emptyFolder" && setup.intent === "create" ? (
          <CreateForm facts={setup.facts} />
        ) : setup.kind === "emptyFolder" ? (
          <CloneForm facts={setup.facts} />
        ) : setup.kind === "import" ? (
          <ImportForm />
        ) : (
          <Panel title={setup.title} onBack={() => void cancelSetup()}>
            <p className="text-sm text-muted">{setup.message}</p>
            <Button className="mt-5" onClick={() => void openWorkspace("open")}>
              Choose a different folder
            </Button>
          </Panel>
        )}
      </div>
    </div>
  );
}

function Start({ busy }: { busy: boolean }) {
  const options = [
    {
      icon: <FolderOpen />,
      title: "Open a Workspace",
      text: "A folder you already use with Rehow, or a folder of Markdown notes to import.",
      run: () => void openWorkspace("open"),
    },
    {
      icon: <FolderPlus />,
      title: "Create a new Workspace",
      text: "Pick an empty folder. Your Guides are saved there as plain Markdown files.",
      run: () => void openWorkspace("create"),
    },
    {
      icon: <GitBranch />,
      title: "Clone from Git",
      text: "Bring your Guides from GitHub, GitLab or your own server into an empty folder.",
      run: () => void openWorkspace("clone"),
    },
  ];
  return (
    <div>
      <motion.div
        initial={{ opacity: 0, scale: 0.9 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.3 }}
        className="mb-6 flex size-11 items-center justify-center rounded-xl bg-accent text-accent-fg shadow-[0_8px_24px_-8px_var(--accent)]"
      >
        <BookOpen className="size-5" strokeWidth={2.2} />
      </motion.div>
      <BlurText
        text="Remember how you did it."
        className="text-[30px] leading-tight font-semibold tracking-[-0.02em]"
      />
      <motion.p
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.35, duration: 0.4 }}
        className="mt-2 text-[15px] text-muted"
      >
        Rehow keeps the commands, fixes and setups you figured out once, so you
        never have to figure them out twice. Everything stays on your machine as
        Markdown.
      </motion.p>
      <ul className="mt-8 space-y-2">
        {options.map((option, index) => (
          <motion.li
            key={option.title}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{
              delay: 0.45 + index * 0.07,
              duration: 0.3,
              ease: "easeOut",
            }}
          >
            <button
              type="button"
              disabled={busy}
              onClick={option.run}
              className="group flex w-full items-center gap-4 rounded-xl border border-border bg-panel px-4 py-3.5 text-left shadow-[0_1px_2px_rgb(0_0_0/0.03)] transition-[border-color,box-shadow] duration-150 hover:border-border-strong hover:shadow-[0_4px_16px_-6px_rgb(0_0_0/0.12)] disabled:opacity-60"
            >
              <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent-text [&_svg]:size-[18px]">
                {option.icon}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-semibold">
                  {option.title}
                </span>
                <span className="block text-xs text-muted">{option.text}</span>
              </span>
              <ChevronRight className="size-4 text-faint transition-transform duration-150 group-hover:translate-x-0.5" />
            </button>
          </motion.li>
        ))}
      </ul>
      <p className="mt-6 text-center text-xs text-faint">
        No account, no cloud. Git and AI assistants are optional.
      </p>
    </div>
  );
}

function Panel({
  title,
  description,
  onBack,
  children,
}: {
  title: string;
  description?: string;
  onBack: () => void;
  children: React.ReactNode;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.18 }}
    >
      <button
        type="button"
        onClick={onBack}
        className="mb-6 inline-flex items-center gap-1.5 text-sm text-muted hover:text-text"
      >
        <ArrowLeft className="size-4" /> Back
      </button>
      <h1 className="text-[22px] font-semibold tracking-[-0.015em]">{title}</h1>
      {description ? (
        <p className="mt-1.5 text-sm text-muted">{description}</p>
      ) : null}
      <div className="mt-6">{children}</div>
    </motion.div>
  );
}

function FolderNote({ facts }: { facts: FolderFacts }) {
  if (
    facts.writable &&
    !facts.existingGitRepository &&
    !facts.nestedGitRepository
  )
    return null;
  return (
    <p className="mt-3 text-xs text-muted">
      {!facts.writable
        ? "This folder is read-only, so Rehow can't save anything there."
        : facts.nestedGitRepository
          ? "This folder is inside another Git repository."
          : "This folder is already a Git repository."}
    </p>
  );
}

function CreateForm({ facts }: { facts: FolderFacts }) {
  const [name, setName] = useState("My Guides");
  const busy = useApp((state) => state.busy !== null);
  return (
    <Panel
      title="Create a Workspace"
      description="Name it something you'll recognize. You can create Categories and Guides next."
      onBack={() => void cancelSetup()}
    >
      <form
        className="space-y-4"
        onSubmit={(event) => {
          event.preventDefault();
          if (name.trim()) void createWorkspace(name.trim());
        }}
      >
        <Field label="Workspace name" htmlFor="workspace-name">
          <Input
            id="workspace-name"
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            maxLength={200}
          />
        </Field>
        <div className="flex items-center gap-2">
          <Button
            type="submit"
            variant="primary"
            disabled={busy || !facts.writable || name.trim() === ""}
          >
            {busy ? "Creating…" : "Create Workspace"}
          </Button>
          <Button
            variant="ghost"
            onClick={() =>
              useApp.setState({
                setup: { kind: "emptyFolder", facts, intent: "clone" },
              })
            }
          >
            Clone from Git instead
          </Button>
        </div>
        <FolderNote facts={facts} />
      </form>
    </Panel>
  );
}

type CloneStage =
  | "entry"
  | "validating"
  | "validated"
  | "completing"
  | "recovery";

function CloneForm({ facts }: { facts: FolderFacts }) {
  const [url, setUrl] = useState("");
  const [stage, setStage] = useState<CloneStage>("entry");
  const [host, setHost] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function validate(event: React.FormEvent) {
    event.preventDefault();
    const remoteUrl = normalizedCloneUrl(url);
    if (remoteUrl === null) {
      setError(
        "Enter a repository address like git@github.com:you/guides.git or https://github.com/you/guides.git (no passwords).",
      );
      return;
    }
    setError(null);
    setStage("validating");
    try {
      const plan = await api.git.clonePlan(remoteUrl);
      setHost(plan.host);
      const result = await api.git.cloneApply(plan.planToken);
      setStage(result.validated ? "validated" : "entry");
    } catch (failure) {
      const native = toNativeError(failure);
      setStage(native.code === "invalidResponse" ? "recovery" : "entry");
      if (native.code !== "cancelled") setError(native.message);
    }
  }

  async function complete() {
    const previous = stage;
    setStage("completing");
    setError(null);
    try {
      const result = await api.git.cloneComplete();
      if (result.kind === "declined") {
        setStage(previous);
        return;
      }
      adoptWorkspace(result.workspace, `Cloned ${result.workspace.name}`);
    } catch (failure) {
      setStage(previous);
      setError(toNativeError(failure).message);
    }
  }

  const working = stage === "validating" || stage === "completing";

  return (
    <Panel
      title="Clone from Git"
      description="Rehow downloads and checks the repository privately first. Your folder isn't touched until you confirm."
      onBack={() => {
        if (stage === "validating")
          void api.git.cloneCancel().catch(() => undefined);
        void cancelSetup();
      }}
    >
      {stage === "validated" ||
      stage === "recovery" ||
      stage === "completing" ? (
        <div className="space-y-4">
          <div className="rounded-lg border border-accent/40 bg-accent-soft px-4 py-3 text-sm">
            <p className="font-medium text-accent-text">
              {stage === "recovery"
                ? "Ready to finish"
                : `Repository from ${host} looks good`}
            </p>
            <p className="mt-0.5 text-muted">
              Copy it into the selected folder to start using it.
            </p>
          </div>
          {error ? (
            <p className="text-sm text-danger" role="alert">
              {error}
            </p>
          ) : null}
          <div className="flex gap-2">
            <Button
              variant="primary"
              onClick={() => void complete()}
              disabled={stage === "completing"}
            >
              {stage === "completing" ? "Finishing…" : "Finish cloning"}
            </Button>
            <Button
              variant="ghost"
              onClick={() => void cancelSetup()}
              disabled={stage === "completing"}
            >
              Cancel
            </Button>
          </div>
        </div>
      ) : (
        <form className="space-y-4" onSubmit={validate}>
          <Field
            label="Repository address"
            htmlFor="clone-url"
            error={error}
            hint="SSH or HTTPS. Rehow uses your existing SSH keys or credential helper."
          >
            <Input
              id="clone-url"
              autoFocus
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              placeholder="git@github.com:you/guides.git"
              spellCheck={false}
              disabled={working}
            />
          </Field>
          <div className="flex items-center gap-2">
            <Button
              type="submit"
              variant="primary"
              disabled={working || url.trim() === "" || !facts.writable}
            >
              {working ? <Loader2 className="animate-spin" /> : null}
              {working ? "Checking repository…" : "Check repository"}
            </Button>
            {stage === "validating" ? (
              <Button
                variant="ghost"
                onClick={() =>
                  void api.git.cloneCancel().catch(() => undefined)
                }
              >
                Stop
              </Button>
            ) : (
              <Button
                variant="ghost"
                onClick={() =>
                  useApp.setState({
                    setup: { kind: "emptyFolder", facts, intent: "create" },
                  })
                }
              >
                Create an empty Workspace instead
              </Button>
            )}
          </div>
          <FolderNote facts={facts} />
        </form>
      )}
    </Panel>
  );
}

function ImportForm() {
  const setup = useApp((state) => state.setup);
  const [workspaceName, setWorkspaceName] = useState("Imported Guides");
  const [categoryName, setCategoryName] = useState("Imported");
  const [busy, setBusy] = useState(false);
  if (setup.kind !== "import") return null;
  const { preview, destination } = setup;
  const skipped = preview.malformedOrUnsafe + preview.oversized;

  async function chooseDestination() {
    try {
      const result = await api.chooseImportDestination();
      if (result.kind === "selected" && setup.kind === "import") {
        useApp.setState({ setup: { ...setup, destination: result.facts } });
      }
    } catch (error) {
      toast.error(toNativeError(error).message);
    }
  }

  async function apply(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      const result = await api.applyMarkdownImport(
        workspaceName.trim(),
        categoryName.trim(),
      );
      adoptWorkspace(
        result.workspace,
        result.complete
          ? `Imported ${plural(result.imported, "Guide")}`
          : `Imported ${result.imported} of ${result.total}`,
      );
      if (!result.complete && result.failure) toast.warning(result.failure);
    } catch (error) {
      toast.error("Import failed", {
        description: toNativeError(error).message,
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Panel
      title="Import Markdown notes"
      description="Rehow copies your notes into a new Workspace. The original folder is never changed."
      onBack={() => void cancelSetup()}
    >
      <div className="grid grid-cols-3 gap-2 text-center">
        {(
          [
            ["Ready to import", preview.included],
            ["Not Markdown", preview.excludedNonMarkdown],
            ["Skipped", skipped],
          ] as const
        ).map(([label, count]) => (
          <div
            key={label}
            className="rounded-lg border border-border bg-panel px-2 py-3"
          >
            <span className="block text-xl font-semibold tabular-nums">
              {count}
            </span>
            <span className="text-xs text-muted">{label}</span>
          </div>
        ))}
      </div>
      {preview.entries.length > 0 ? (
        <details className="mt-4 rounded-lg border border-border bg-panel">
          <summary className="cursor-default px-3 py-2 text-sm text-muted">
            Show files
          </summary>
          <ul className="max-h-48 overflow-y-auto border-t border-border px-3 py-2 font-mono text-xs text-muted">
            {preview.entries.map((entry) => (
              <li key={entry.displayPath} className="truncate py-0.5">
                {entry.displayPath}
              </li>
            ))}
          </ul>
        </details>
      ) : null}

      <ol className="mt-6 space-y-4">
        <li
          className={cn(
            "rounded-lg border px-4 py-3",
            destination ? "border-border" : "border-accent/50",
          )}
        >
          <p className="text-sm font-medium">
            1. Choose an empty folder for the new Workspace
          </p>
          <div className="mt-2 flex items-center gap-2">
            <Button size="sm" onClick={() => void chooseDestination()}>
              <FileStack />{" "}
              {destination ? "Choose a different folder" : "Choose folder…"}
            </Button>
            {destination ? (
              <span className="text-xs text-accent-text">Folder selected</span>
            ) : null}
          </div>
        </li>
        <li
          className={cn(
            "rounded-lg border px-4 py-3",
            destination ? "border-accent/50" : "border-border opacity-60",
          )}
        >
          <form className="space-y-3" onSubmit={apply}>
            <p className="text-sm font-medium">2. Name it</p>
            <div className="grid gap-3 sm:grid-cols-2">
              <Field label="Workspace name" htmlFor="import-workspace">
                <Input
                  id="import-workspace"
                  value={workspaceName}
                  onChange={(event) => setWorkspaceName(event.target.value)}
                  disabled={!destination}
                />
              </Field>
              <Field label="Put Guides in Category" htmlFor="import-category">
                <Input
                  id="import-category"
                  value={categoryName}
                  onChange={(event) => setCategoryName(event.target.value)}
                  disabled={!destination}
                />
              </Field>
            </div>
            <Button
              type="submit"
              variant="primary"
              disabled={
                !destination ||
                busy ||
                workspaceName.trim() === "" ||
                categoryName.trim() === ""
              }
            >
              {busy
                ? "Importing…"
                : `Import ${plural(preview.included, "note")}`}
            </Button>
          </form>
        </li>
      </ol>
    </Panel>
  );
}
