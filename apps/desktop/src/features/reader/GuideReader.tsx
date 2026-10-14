import {
  AlertTriangle,
  Archive,
  ArchiveRestore,
  ArrowLeft,
  BadgeCheck,
  ChevronRight,
  ExternalLink,
  FileCode2,
  FileText,
  FolderOpen,
  Link2,
  ListChecks,
  MoreHorizontal,
  Paperclip,
  Pencil,
  Trash2,
} from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { api } from "@/api/client";
import type {
  AttachmentTextPreviewPayload,
  GuideBodyPayload,
  GuideSummary,
} from "@/api/payloads";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
} from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { StatusBadge } from "@/components/ui/status";
import { Tooltip } from "@/components/ui/tooltip";
import {
  formatBytes,
  formatLongDate,
  formatRelative,
  modKey,
} from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  deleteGuide,
  exportAttachment,
  filterByCategory,
  filterByTag,
  importAttachment,
  markTested,
  openDialog,
  openExternalLink,
  openGuideFile,
  removeAttachment,
  reportError,
  revealAttachment,
  selectGuide,
  setGuideArchived,
  startEditing,
  useApp,
} from "@/store/app";
import {
  extractOutline,
  MarkdownDocument,
  type OutlineEntry,
} from "./MarkdownDocument";

export function GuideReader({ onBack }: { onBack?: () => void }) {
  const guide = useApp((state) => state.guide);
  const loading = useApp((state) => state.guideLoading);
  const summary = useApp(
    (state) =>
      state.workspace?.guides.find((item) => item.id === state.selectedId) ??
      null,
  );
  const hasGuides = useApp(
    (state) => (state.workspace?.guides.length ?? 0) > 0,
  );

  if (summary === null || (guide === null && !loading)) {
    return <ReaderEmpty hasGuides={hasGuides} />;
  }
  if (guide === null || guide.id !== summary.id) {
    return <div className="flex-1" aria-busy="true" />;
  }
  return (
    <ReaderContent
      key={guide.id}
      guide={guide}
      summary={summary}
      onBack={onBack}
    />
  );
}

function ReaderEmpty({ hasGuides }: { hasGuides: boolean }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
      <FileText className="size-8 text-border-strong" strokeWidth={1.5} />
      <p className="text-sm text-muted">
        {hasGuides
          ? "Select a Guide to read it."
          : "Your Guides will appear here."}
      </p>
      <p className="flex items-center gap-1 text-xs text-faint">
        Press <Kbd>{modKey}</Kbd>
        <Kbd>K</Kbd> to search or run a command
      </p>
    </div>
  );
}

function ReaderContent({
  guide,
  summary,
  onBack,
}: {
  guide: GuideBodyPayload;
  summary: GuideSummary;
  onBack?: () => void;
}) {
  const writable = useApp((state) => state.workspace?.facts.writable ?? false);
  const outline = useMemo(
    () => extractOutline(guide.document),
    [guide.document],
  );
  const archived = summary.status === "Archived";
  const canMarkTested =
    summary.status === "Draft" || summary.status === "Stale";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-1 border-b border-border px-3">
        {onBack ? (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Back to Guides"
            onClick={onBack}
          >
            <ArrowLeft />
          </Button>
        ) : null}
        <nav
          aria-label="Breadcrumb"
          className="flex min-w-0 flex-1 items-center gap-1 text-xs text-muted"
        >
          <button
            type="button"
            className="max-w-[45%] shrink-0 truncate rounded px-1 py-0.5 hover:bg-hover hover:text-text"
            onClick={() => filterByCategory(summary.categoryId)}
          >
            {summary.category}
          </button>
          <ChevronRight className="size-3 shrink-0 text-faint" />
          <span className="truncate text-text">{summary.title}</span>
        </nav>
        {canMarkTested && writable ? (
          <Tooltip label="Confirm you followed these steps and they work">
            <Button variant="ghost" size="sm" onClick={() => void markTested()}>
              <BadgeCheck /> Mark as tested
            </Button>
          </Tooltip>
        ) : null}
        <Tooltip label="Edit Guide" shortcut="E">
          <Button size="sm" onClick={startEditing} disabled={!writable}>
            <Pencil /> Edit
          </Button>
        </Tooltip>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label="More actions">
              <MoreHorizontal />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            <DropdownMenuItem onSelect={() => void openGuideFile()}>
              <FileCode2 /> Open in text editor
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!writable}
              onSelect={() => void importAttachment()}
            >
              <Paperclip /> Attach a file…
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              disabled={!writable}
              onSelect={() => void setGuideArchived(!archived)}
            >
              {archived ? <ArchiveRestore /> : <Archive />}
              {archived ? "Restore from archive" : "Archive"}
            </DropdownMenuItem>
            <DropdownMenuItem
              destructive
              disabled={!writable}
              onSelect={() => void deleteGuide()}
            >
              <Trash2 /> Delete…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <div
        id="guide-reader"
        tabIndex={-1}
        className="min-h-0 flex-1 overflow-y-auto outline-none [container-type:inline-size]"
      >
        <motion.div
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.14, ease: "easeOut" }}
          className="mx-auto flex max-w-[1080px] gap-10 px-8 pt-8 pb-24 @max-[640px]:px-5"
        >
          <article className="min-w-0 max-w-[740px] flex-1">
            <header className="mb-6 space-y-3">
              <h1 className="text-[26px] leading-[1.25] font-semibold tracking-[-0.015em] text-balance">
                {guide.title}
              </h1>
              <p className="text-[15px] leading-6 text-muted text-pretty">
                {guide.summary}
              </p>
              <div className="flex flex-wrap items-center gap-x-3 gap-y-2 pt-1 text-xs text-muted">
                <StatusBadge status={summary.status} />
                <span title={formatLongDate(summary.updatedAt)}>
                  Updated {formatRelative(summary.updatedAt)}
                </span>
                {guide.lastVerifiedAt ? (
                  <span title={formatLongDate(guide.lastVerifiedAt)}>
                    Last tested {formatRelative(guide.lastVerifiedAt)}
                  </span>
                ) : null}
                {guide.tags.length > 0 ? (
                  <span className="flex flex-wrap gap-1">
                    {guide.tags.map((tag) => (
                      <button
                        key={tag}
                        type="button"
                        onClick={() => filterByTag(tag)}
                        className="rounded-md bg-hover px-1.5 py-0.5 text-xs text-muted hover:bg-active hover:text-text"
                      >
                        #{tag}
                      </button>
                    ))}
                  </span>
                ) : null}
              </div>
              {guide.aliases.length > 0 ? (
                <p className="text-xs text-faint">
                  Also known as:{" "}
                  <span className="text-muted">{guide.aliases.join(", ")}</span>
                </p>
              ) : null}
            </header>

            {summary.status === "Stale" ? (
              <Callout
                tone="warning"
                icon={<AlertTriangle />}
                title="Changed since it was last tested"
              >
                Follow the steps again, then{" "}
                <button
                  type="button"
                  className="font-medium underline underline-offset-2"
                  onClick={() => void markTested()}
                >
                  mark it as tested
                </button>{" "}
                so you know it's still reliable.
              </Callout>
            ) : null}

            <GuideContext guide={guide} />

            <MarkdownDocument
              document={guide.document}
              onOpenLink={(url) => void openExternalLink(url)}
            />

            {summary.attachments.length > 0 ? (
              <Attachments
                guide={guide}
                summary={summary}
                writable={writable}
              />
            ) : null}
          </article>
          {outline.length >= 3 ? <Outline outline={outline} /> : null}
        </motion.div>
      </div>
    </div>
  );
}

/** Plain-text metadata with `backtick` spans shown as code. Never parses HTML. */
function InlineText({ text }: { text: string }) {
  const parts = text.split(/(`[^`]+`)/g);
  return (
    <>
      {parts.map((part, index) =>
        part.startsWith("`") && part.endsWith("`") && part.length > 2 ? (
          <code
            // biome-ignore lint/suspicious/noArrayIndexKey: parts are positional.
            key={index}
            className="rounded bg-code px-1 py-px font-mono text-[0.88em] text-text"
          >
            {part.slice(1, -1)}
          </code>
        ) : (
          part
        ),
      )}
    </>
  );
}

function Callout({
  tone,
  icon,
  title,
  children,
}: {
  tone: "warning" | "info";
  icon: React.ReactNode;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <aside
      className={cn(
        "mb-6 flex gap-3 rounded-lg border px-4 py-3 text-sm [&>svg]:mt-0.5 [&>svg]:size-4 [&>svg]:shrink-0",
        tone === "warning"
          ? "border-warning-border bg-warning-soft text-text [&>svg]:text-warning"
          : "border-border bg-info-soft [&>svg]:text-info",
      )}
    >
      {icon}
      <div className="min-w-0 space-y-1">
        <p className="font-semibold">{title}</p>
        <div className="text-muted [&_li]:mt-1">{children}</div>
      </div>
    </aside>
  );
}

function ContextBlock({
  title,
  icon,
  children,
}: {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="min-w-0">
      <h2 className="mb-2 flex items-center gap-1.5 text-2xs font-semibold text-faint uppercase tracking-[0.06em] [&>svg]:size-3.5">
        {icon}
        {title}
      </h2>
      {children}
    </section>
  );
}

function GuideContext({ guide }: { guide: GuideBodyPayload }) {
  const guides = useApp((state) => state.workspace?.guides ?? []);
  const blocks = [
    guide.prerequisites.length > 0 ? (
      <ContextBlock key="pre" title="Before you start" icon={<ListChecks />}>
        <ul className="space-y-1.5 text-sm">
          {guide.prerequisites.map((item) => (
            <li key={item} className="flex gap-2">
              <span
                aria-hidden="true"
                className="mt-[7px] size-1.5 shrink-0 rounded-full bg-border-strong"
              />
              <span>
                <InlineText text={item} />
              </span>
            </li>
          ))}
        </ul>
      </ContextBlock>
    ) : null,
    guide.appliesTo.length > 0 ? (
      <ContextBlock key="applies" title="Applies to" icon={<BadgeCheck />}>
        <ul className="space-y-1.5 text-sm">
          {guide.appliesTo.map((item) => (
            <li key={`${item.platform}-${item.versions}-${item.notes}`}>
              <span className="font-medium">{item.platform}</span>
              {item.versions ? (
                <span className="text-muted"> · {item.versions}</span>
              ) : null}
              {item.notes ? (
                <span className="block text-xs text-faint">{item.notes}</span>
              ) : null}
            </li>
          ))}
        </ul>
      </ContextBlock>
    ) : null,
    guide.filesAffected.length > 0 ? (
      <ContextBlock key="files" title="Files it changes" icon={<FolderOpen />}>
        <ul className="space-y-1">
          {guide.filesAffected.map((file) => (
            <li key={file}>
              <code className="font-mono text-xs break-all text-text">
                {file}
              </code>
            </li>
          ))}
        </ul>
      </ContextBlock>
    ) : null,
    guide.relatedGuideIds.length > 0 ? (
      <ContextBlock key="related" title="Related" icon={<Link2 />}>
        <ul className="space-y-1 text-sm">
          {guide.relatedGuideIds.map((id) => {
            const related = guides.find((item) => item.id === id);
            return (
              <li key={id}>
                {related ? (
                  <button
                    type="button"
                    className="text-left text-accent-text hover:underline"
                    onClick={() => {
                      useApp.setState({
                        query: "",
                        scope: "all",
                        categoryFilter: null,
                        tagFilter: null,
                      });
                      void selectGuide(id);
                    }}
                  >
                    {related.title}
                  </button>
                ) : (
                  <span className="text-faint">
                    A Guide that is no longer available
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      </ContextBlock>
    ) : null,
    guide.references.length > 0 ? (
      <ContextBlock key="refs" title="References" icon={<ExternalLink />}>
        <ul className="space-y-1 text-sm">
          {guide.references.map((reference) => (
            <li key={reference.url}>
              <button
                type="button"
                title={reference.url}
                className="text-left text-accent-text hover:underline"
                onClick={() => void openExternalLink(reference.url)}
              >
                {reference.title}
              </button>
            </li>
          ))}
        </ul>
      </ContextBlock>
    ) : null,
  ].filter(Boolean);

  return (
    <>
      {guide.warnings.length > 0 ? (
        <Callout
          tone="warning"
          icon={<AlertTriangle />}
          title={guide.warnings.length === 1 ? "Warning" : "Warnings"}
        >
          {guide.warnings.length === 1 ? (
            <p>
              <InlineText text={guide.warnings[0] ?? ""} />
            </p>
          ) : (
            <ul className="list-disc pl-4">
              {guide.warnings.map((warning) => (
                <li key={warning}>
                  <InlineText text={warning} />
                </li>
              ))}
            </ul>
          )}
        </Callout>
      ) : null}
      {blocks.length > 0 ? (
        <div className="mb-8 grid gap-x-8 gap-y-5 rounded-lg border border-border bg-bg/60 p-4 @min-[560px]:grid-cols-2">
          {blocks}
        </div>
      ) : null}
    </>
  );
}

function Outline({ outline }: { outline: OutlineEntry[] }) {
  const [active, setActive] = useState(outline[0]?.id ?? "");
  useEffect(() => {
    const root = document.getElementById("guide-reader");
    if (!root) return;
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.filter((entry) => entry.isIntersecting);
        const first = visible.sort(
          (a, b) => a.boundingClientRect.top - b.boundingClientRect.top,
        )[0];
        if (first) setActive(first.target.id);
      },
      { root, rootMargin: "0px 0px -70% 0px" },
    );
    for (const entry of outline) {
      const element = document.getElementById(entry.id);
      if (element) observer.observe(element);
    }
    return () => observer.disconnect();
  }, [outline]);
  const minLevel = Math.min(...outline.map((entry) => entry.level));

  return (
    <nav
      aria-label="On this page"
      className="sticky top-8 hidden w-48 shrink-0 self-start @min-[1000px]:block"
    >
      <p className="mb-2 text-2xs font-semibold text-faint uppercase tracking-[0.06em]">
        On this page
      </p>
      <ul className="space-y-0.5 border-l border-border">
        {outline.map((entry) => (
          <li key={entry.id}>
            <a
              href={`#${entry.id}`}
              onClick={(event) => {
                event.preventDefault();
                document
                  .getElementById(entry.id)
                  ?.scrollIntoView({ behavior: "smooth", block: "start" });
                setActive(entry.id);
              }}
              style={{ paddingLeft: `${(entry.level - minLevel) * 10 + 12}px` }}
              className={cn(
                "-ml-px block truncate border-l py-1 pr-2 text-xs",
                active === entry.id
                  ? "border-accent font-medium text-text"
                  : "border-transparent text-muted hover:text-text",
              )}
            >
              {entry.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}

function Attachments({
  guide,
  summary,
  writable,
}: {
  guide: GuideBodyPayload;
  summary: GuideSummary;
  writable: boolean;
}) {
  const session = useApp((state) => state.workspace?.watchSession ?? "");
  const [preview, setPreview] = useState<AttachmentTextPreviewPayload | null>(
    null,
  );

  async function openPreview(attachmentId: string) {
    try {
      setPreview(
        await api.previewAttachmentText(
          guide.id,
          attachmentId,
          guide.revision,
          session,
        ),
      );
    } catch (error) {
      reportError(error, "This attachment can't be previewed");
    }
  }

  return (
    <section className="mt-12" aria-labelledby="attachments-heading">
      <h2
        id="attachments-heading"
        className="mb-3 flex items-center gap-2 text-sm font-semibold"
      >
        <Paperclip className="size-4 text-faint" /> Attachments
      </h2>
      <ul className="divide-y divide-border overflow-hidden rounded-lg border border-border">
        {summary.attachments.map((attachment) => (
          <li key={attachment.id} className="flex items-center gap-3 px-3 py-2">
            <FileText className="size-4 shrink-0 text-faint" />
            <button
              type="button"
              className="min-w-0 flex-1 truncate text-left text-sm hover:underline"
              onClick={() => void openPreview(attachment.id)}
            >
              {attachment.title}
            </button>
            <span className="text-xs text-faint tabular-nums">
              {formatBytes(attachment.sizeBytes)}
            </span>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`${attachment.title} options`}
                >
                  <MoreHorizontal />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  onSelect={() => void openPreview(attachment.id)}
                >
                  <FileText /> Preview
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => void exportAttachment(attachment.id)}
                >
                  <ExternalLink /> Save a copy…
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => void revealAttachment(attachment.id)}
                >
                  <FolderOpen /> Show in folder
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  destructive
                  disabled={!writable}
                  onSelect={() =>
                    void removeAttachment(attachment.id, attachment.title)
                  }
                >
                  <Trash2 /> Remove…
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </li>
        ))}
      </ul>
      <Dialog
        open={preview !== null}
        onOpenChange={(open) => !open && setPreview(null)}
      >
        <DialogContent className="w-[min(92vw,760px)]">
          <DialogHeader
            title={preview?.title ?? ""}
            description={
              preview?.sanitized
                ? "Some invisible or control characters were hidden for safety."
                : formatBytes(preview?.sizeBytes ?? null)
            }
          />
          <DialogBody className="pb-5">
            <pre className="max-h-[60vh] overflow-auto rounded-lg border border-border bg-code p-4 font-mono text-xs leading-5 whitespace-pre-wrap">
              {preview?.text}
            </pre>
          </DialogBody>
        </DialogContent>
      </Dialog>
    </section>
  );
}

export function openNewGuide() {
  openDialog("newGuide");
}
