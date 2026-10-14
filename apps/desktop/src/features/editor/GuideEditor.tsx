import type { SafeMarkdownDocument } from "@rehow/contracts";
import {
  AlertTriangle,
  Bold,
  CheckSquare,
  Code,
  Columns2,
  Eye,
  FileCode2,
  Heading2,
  Italic,
  Link as LinkIcon,
  List,
  ListOrdered,
  ListPlus,
  PenLine,
  RotateCcw,
  SquareTerminal,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { api } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { Select } from "@/components/ui/select";
import { Tooltip } from "@/components/ui/tooltip";
import { MarkdownDocument } from "@/features/reader/MarkdownDocument";
import { confirm } from "@/lib/confirm";
import { modKey } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  cancelEditing,
  clearStoredDraft,
  draftIsDirty,
  readStoredDraft,
  refreshWorkspace,
  saveDraft,
  updateDraft,
  useApp,
} from "@/store/app";
import { MarkdownEditor, type MarkdownEditorHandle } from "./MarkdownEditor";
import { TagInput } from "./TagInput";

type View = "write" | "split" | "preview";

const SECTIONS: Array<{ label: string; text: string }> = [
  {
    label: "Steps",
    text: "## Steps\n\n1. \n",
  },
  {
    label: "Command",
    text: "```bash\n\n```",
  },
  {
    label: "Verify it worked",
    text: "## Verify\n\nRun this and check that …\n\n```bash\n\n```",
  },
  {
    label: "Undo / rollback",
    text: "## Rollback\n\n- \n",
  },
  {
    label: "Troubleshooting",
    text: "## Troubleshooting\n\n| Symptom | Fix |\n| --- | --- |\n|  |  |",
  },
];

const VIEW_KEY = "rehow:editor-view";

export function GuideEditor() {
  const draft = useApp((state) => state.draft);
  const guide = useApp((state) => state.guide);
  const saving = useApp((state) => state.saving);
  const externalChange = useApp((state) => state.externalChange);
  const workspace = useApp((state) => state.workspace);
  const dirty = useApp((state) => draftIsDirty(state));
  const editor = useRef<MarkdownEditorHandle>(null);
  const [editorKey, setEditorKey] = useState(0);
  const [view, setView] = useState<View>(() => {
    try {
      const stored = localStorage.getItem(VIEW_KEY);
      if (stored === "write" || stored === "split" || stored === "preview")
        return stored;
    } catch {
      // Default below.
    }
    return "split";
  });
  const [restorable, setRestorable] =
    useState<ReturnType<typeof readStoredDraft>>(null);

  useEffect(() => {
    try {
      localStorage.setItem(VIEW_KEY, view);
    } catch {
      // Not important.
    }
  }, [view]);

  // Offer to restore a draft left behind by a crash or reload.
  useEffect(() => {
    if (!guide) return;
    const stored = readStoredDraft(guide.id);
    if (
      stored &&
      stored.baseRevision === guide.revision &&
      stored.draft.body !== guide.body
    ) {
      setRestorable(stored);
    }
  }, [guide]);

  useEffect(() => {
    requestAnimationFrame(() =>
      document.getElementById("draft-title")?.focus(),
    );
  }, []);

  const tagSuggestions = useMemo(() => {
    const all = new Set<string>();
    for (const item of workspace?.guides ?? [])
      for (const tag of item.tags) all.add(tag);
    return [...all].sort();
  }, [workspace]);

  if (draft === null || guide === null || workspace === null) return null;
  const categories = workspace.categories.filter(
    (category) =>
      category.status === "active" || category.id === draft.categoryId,
  );
  const wasTested =
    workspace.guides.find((item) => item.id === guide.id)?.status === "Tested";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border px-3">
        <span className="flex items-center gap-2 text-xs text-muted">
          <PenLine className="size-3.5" />
          Editing
          {dirty ? (
            <span className="flex items-center gap-1.5 text-warning">
              <span className="size-1.5 rounded-full bg-warning" /> Unsaved
            </span>
          ) : (
            <span className="text-faint">· No changes</span>
          )}
        </span>
        <div className="flex-1" />
        <fieldset className="flex rounded-md border border-border bg-bg p-0.5">
          <legend className="sr-only">Editor layout</legend>
          {(
            [
              ["write", "Write", PenLine],
              ["split", "Split", Columns2],
              ["preview", "Preview", Eye],
            ] as const
          ).map(([value, label, Icon]) => (
            <Tooltip key={value} label={label}>
              <button
                type="button"
                aria-pressed={view === value}
                aria-label={label}
                onClick={() => setView(value)}
                className={cn(
                  "inline-flex h-6 w-7 items-center justify-center rounded text-muted",
                  view === value
                    ? "bg-panel text-text shadow-[0_0_0_1px_var(--border)]"
                    : "hover:text-text",
                )}
              >
                <Icon className="size-3.5" />
              </button>
            </Tooltip>
          ))}
        </fieldset>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void cancelEditing()}
          disabled={saving}
        >
          {dirty ? "Discard" : "Done"}
        </Button>
        <Tooltip label="Save" shortcut={`${modKey}+S`}>
          <Button
            variant="primary"
            size="sm"
            onClick={() => void saveDraft()}
            disabled={saving || !dirty}
          >
            {saving ? "Saving…" : "Save"}
          </Button>
        </Tooltip>
      </div>

      {externalChange ? (
        <ConflictBanner
          onReload={async () => {
            const ok = await confirm({
              title: "Discard your edits and reload?",
              description:
                "The version on disk will replace your unsaved changes.",
              confirmLabel: "Reload from disk",
              destructive: true,
            });
            if (!ok) return;
            clearStoredDraft(draft.guideId);
            useApp.setState({ draft: null, externalChange: false });
            await refreshWorkspace();
          }}
        />
      ) : null}
      {restorable ? (
        <div className="flex items-center gap-3 border-b border-info/30 bg-info-soft px-4 py-2 text-sm">
          <RotateCcw className="size-4 shrink-0 text-info" />
          <span className="flex-1">
            You have unsaved changes from a previous session.
          </span>
          <Button
            size="sm"
            onClick={() => {
              const { guideId: _guideId, ...restored } = restorable.draft;
              updateDraft(restored);
              setEditorKey((key) => key + 1);
              setRestorable(null);
            }}
          >
            Restore
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              clearStoredDraft(guide.id);
              setRestorable(null);
            }}
          >
            Dismiss
          </Button>
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="border-b border-border px-6 pt-5 pb-4 [container-type:inline-size]">
          <input
            id="draft-title"
            value={draft.title}
            onChange={(event) => updateDraft({ title: event.target.value })}
            placeholder="Guide title"
            aria-label="Title"
            maxLength={200}
            className="w-full bg-transparent text-[22px] leading-8 font-semibold tracking-[-0.01em] outline-none placeholder:text-faint"
          />
          <input
            value={draft.summary}
            onChange={(event) => updateDraft({ summary: event.target.value })}
            placeholder="One line: what this Guide helps you do"
            aria-label="Summary"
            maxLength={500}
            className="mt-1 w-full bg-transparent text-[15px] leading-6 text-muted outline-none placeholder:text-faint"
          />
          <div className="mt-4 grid gap-3 @min-[720px]:grid-cols-[200px_1fr_1fr]">
            <div className="grid gap-1">
              <label
                htmlFor="draft-category"
                className="text-2xs font-medium text-faint uppercase tracking-wide"
              >
                Category
              </label>
              <Select
                id="draft-category"
                value={draft.categoryId}
                onChange={(event) =>
                  updateDraft({ categoryId: event.target.value })
                }
              >
                {categories.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}
                  </option>
                ))}
              </Select>
            </div>
            <div className="grid gap-1">
              <span className="text-2xs font-medium text-faint uppercase tracking-wide">
                Tags
              </span>
              <TagInput
                value={draft.tags}
                onChange={(tags) => updateDraft({ tags })}
                suggestions={tagSuggestions}
                placeholder="docker, networking…"
                ariaLabel="Tags"
                prefix="#"
                maxLength={64}
              />
            </div>
            <div className="grid gap-1">
              <span className="text-2xs font-medium text-faint uppercase tracking-wide">
                Also known as
              </span>
              <TagInput
                value={draft.aliases}
                onChange={(aliases) => updateDraft({ aliases })}
                placeholder="Other names, error messages…"
                ariaLabel="Aliases"
                maxLength={200}
              />
            </div>
          </div>
          {wasTested && dirty ? (
            <p className="mt-3 flex items-center gap-1.5 text-xs text-warning">
              <AlertTriangle className="size-3.5" />
              Saving changes marks this tested Guide as “Needs retest”.
            </p>
          ) : null}
        </div>

        <div
          className={cn(
            "flex min-h-[560px]",
            view === "split" ? "divide-x divide-border" : "",
          )}
        >
          {view !== "preview" ? (
            <div className="flex min-w-0 flex-1 flex-col">
              <Toolbar editor={editor} />
              <div className="min-h-0 flex-1">
                <MarkdownEditor
                  key={editorKey}
                  ref={editor}
                  value={draft.body}
                  onChange={(body) => updateDraft({ body })}
                  ariaLabel="Guide content (Markdown)"
                />
              </div>
            </div>
          ) : null}
          {view !== "write" ? (
            <Preview body={draft.body} initial={guide.document} />
          ) : null}
        </div>
      </div>
      <div className="flex h-7 shrink-0 items-center gap-4 border-t border-border bg-sidebar px-3 text-2xs text-faint">
        <span>Markdown</span>
        <span className="flex items-center gap-1">
          <Kbd>{modKey}</Kbd>
          <Kbd>S</Kbd> save
        </span>
        <span className="flex items-center gap-1">
          <Kbd>{modKey}</Kbd>
          <Kbd>F</Kbd> find in Guide
        </span>
        <span className="ml-auto tabular-nums">
          {draft.body.split(/\s+/).filter(Boolean).length} words
        </span>
      </div>
    </div>
  );
}

function Toolbar({
  editor,
}: {
  editor: React.RefObject<MarkdownEditorHandle | null>;
}) {
  const tools: Array<{
    label: string;
    icon: React.ReactNode;
    run: () => void;
  }> = [
    {
      label: "Heading",
      icon: <Heading2 />,
      run: () => editor.current?.prefixLines("## "),
    },
    { label: "Bold", icon: <Bold />, run: () => editor.current?.wrap("**") },
    { label: "Italic", icon: <Italic />, run: () => editor.current?.wrap("_") },
    {
      label: "Inline code",
      icon: <Code />,
      run: () => editor.current?.wrap("`", "`", "code"),
    },
    {
      label: "Command block",
      icon: <SquareTerminal />,
      run: () => editor.current?.insertBlock("```bash\n\n```"),
    },
    {
      label: "Bulleted list",
      icon: <List />,
      run: () => editor.current?.prefixLines("- "),
    },
    {
      label: "Numbered list",
      icon: <ListOrdered />,
      run: () => editor.current?.prefixLines("1. "),
    },
    {
      label: "Checklist",
      icon: <CheckSquare />,
      run: () => editor.current?.prefixLines("- [ ] "),
    },
    {
      label: "Link",
      icon: <LinkIcon />,
      run: () => editor.current?.wrap("[", "](https://)", "link text"),
    },
  ];
  return (
    <div
      className="flex h-9 shrink-0 items-center gap-0.5 border-b border-border px-2"
      role="toolbar"
      aria-label="Formatting"
    >
      {tools.map((tool) => (
        <Tooltip key={tool.label} label={tool.label}>
          <button
            type="button"
            aria-label={tool.label}
            onMouseDown={(event) => event.preventDefault()}
            onClick={tool.run}
            className="inline-flex size-7 items-center justify-center rounded-md text-muted hover:bg-hover hover:text-text [&_svg]:size-4"
          >
            {tool.icon}
          </button>
        </Tooltip>
      ))}
      <span className="mx-1 h-4 w-px bg-border" />
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-xs text-muted hover:bg-hover hover:text-text"
          >
            <ListPlus className="size-4" /> Insert section
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          {SECTIONS.map((section) => (
            <DropdownMenuItem
              key={section.label}
              onSelect={() => editor.current?.insertBlock(section.text)}
            >
              <FileCode2 /> {section.label}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function Preview({
  body,
  initial,
}: {
  body: string;
  initial: SafeMarkdownDocument;
}) {
  const [document, setDocument] = useState<SafeMarkdownDocument | null>(
    initial,
  );
  useEffect(() => {
    let active = true;
    const timer = window.setTimeout(async () => {
      try {
        const rendered = await api.renderPreview(body);
        if (active) setDocument(rendered);
      } catch {
        // Keep the last good preview while the content is temporarily invalid.
      }
    }, 160);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [body]);

  return (
    <div className="min-w-0 flex-1 bg-panel">
      <div className="flex h-9 items-center border-b border-border px-4 text-2xs font-medium text-faint uppercase tracking-wide">
        Preview
      </div>
      <div className="px-6 py-5">
        {document ? (
          <MarkdownDocument
            document={document}
            onOpenLink={() =>
              toast.info("Links open from the reader after saving.")
            }
          />
        ) : null}
      </div>
    </div>
  );
}

function ConflictBanner({ onReload }: { onReload: () => void }) {
  const guide = useApp((state) => state.guide);
  const draft = useApp((state) => state.draft);
  const [comparing, setComparing] = useState<{
    title: string;
    body: string;
  } | null>(null);

  // Adopt the disk revision as the new base while keeping the draft, so the
  // next save is a deliberate replacement rather than a silent overwrite.
  async function keepMine() {
    if (!guide) return;
    try {
      const disk = await api.readGuideFromDisk(guide.id);
      useApp.setState({ guide: disk, externalChange: false });
      toast.info("Keeping your draft", {
        description: "Save when ready to replace the version on disk.",
      });
    } catch (error) {
      toast.error("Couldn't read the file on disk", {
        description: error instanceof Error ? error.message : undefined,
      });
    }
  }

  async function compare() {
    if (!guide) return;
    try {
      const disk = await api.readGuideFromDisk(guide.id);
      setComparing({ title: disk.title, body: disk.body });
    } catch (error) {
      toast.error("Couldn't read the file on disk", {
        description: error instanceof Error ? error.message : undefined,
      });
    }
  }

  return (
    <div className="border-b border-warning-border bg-warning-soft">
      <div className="flex flex-wrap items-center gap-3 px-4 py-2 text-sm">
        <AlertTriangle className="size-4 shrink-0 text-warning" />
        <span className="flex-1">
          <strong className="font-semibold">This Guide changed on disk</strong>
          <span className="text-muted">
            {" "}
            while you were editing. Your draft is safe.
          </span>
        </span>
        <Button
          size="sm"
          onClick={() => (comparing ? setComparing(null) : void compare())}
        >
          {comparing ? "Hide comparison" : "Compare"}
        </Button>
        <Tooltip label="Keep editing your draft; saving will replace the disk version">
          <Button size="sm" onClick={() => void keepMine()}>
            Keep my version
          </Button>
        </Tooltip>
        <Button size="sm" variant="ghost" onClick={onReload}>
          Use disk version
        </Button>
      </div>
      {comparing && draft ? (
        <div className="grid grid-cols-2 gap-3 px-4 pb-3">
          {(
            [
              ["Your draft", draft.body],
              ["On disk", comparing.body],
            ] as const
          ).map(([label, text]) => (
            <div key={label} className="min-w-0">
              <p className="mb-1 text-2xs font-semibold text-faint uppercase tracking-wide">
                {label}
              </p>
              <pre className="max-h-64 overflow-auto rounded-md border border-border bg-panel p-3 font-mono text-xs leading-5 whitespace-pre-wrap">
                {text}
              </pre>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
