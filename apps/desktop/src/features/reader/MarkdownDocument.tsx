// Renders the backend's safe Markdown event stream. There is no raw HTML path:
// HTML arrives as literal text, links open only through the native chooser,
// and images are never fetched.
import type {
  SafeMarkdownContainer,
  SafeMarkdownDocument,
  SafeMarkdownEvent,
} from "@rehow/contracts";
import { Check, Copy, ImageOff } from "lucide-react";
import { isValidElement, type ReactNode, useState } from "react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";

type Frame = {
  node: SafeMarkdownContainer | null;
  children: ReactNode[];
  text: string;
  key: number;
};

type Context = {
  onOpenLink: (destination: string) => void;
  slugs: Map<string, number>;
};

export type OutlineEntry = { id: string; level: number; text: string };

export function MarkdownDocument({
  document,
  onOpenLink,
  className,
}: {
  document: SafeMarkdownDocument;
  onOpenLink: (destination: string) => void;
  className?: string;
}) {
  if (document.version !== 1) {
    return (
      <p role="alert" className="text-sm text-danger">
        This Guide uses a content version this Rehow can't display.
      </p>
    );
  }
  return (
    <div className={cn("prose-guide", className)}>
      {renderEvents(document.events, { onOpenLink, slugs: new Map() })}
    </div>
  );
}

function slugify(text: string, slugs: Map<string, number>) {
  const base =
    text
      .toLocaleLowerCase()
      .trim()
      .replace(/[^\p{L}\p{N}]+/gu, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 64) || "section";
  const seen = slugs.get(base) ?? 0;
  slugs.set(base, seen + 1);
  return `h-${seen === 0 ? base : `${base}-${seen}`}`;
}

/** Headings in document order, with the same ids the renderer assigns. */
export function extractOutline(document: SafeMarkdownDocument): OutlineEntry[] {
  const outline: OutlineEntry[] = [];
  const slugs = new Map<string, number>();
  let depth = 0;
  let current: { level: number; text: string } | null = null;
  for (const event of document.events) {
    if (event.kind === "start") {
      depth++;
      if (event.node.kind === "heading" && depth === 1) {
        current = { level: event.node.level, text: "" };
      }
    } else if (event.kind === "end") {
      depth--;
      if (depth === 0 && current !== null) {
        const text = current.text.trim();
        outline.push({ id: slugify(text, slugs), level: current.level, text });
        current = null;
      }
    } else if (
      current !== null &&
      (event.kind === "text" ||
        event.kind === "inlineCode" ||
        event.kind === "htmlLiteral")
    ) {
      current.text += event.value;
    }
  }
  return outline;
}

export function renderEvents(events: SafeMarkdownEvent[], context: Context) {
  const frames: Frame[] = [{ node: null, children: [], text: "", key: 0 }];
  let key = 1;
  const closeFrame = () => {
    const completed = frames.pop();
    const parent = frames.at(-1);
    if (completed && completed.node !== null && parent) {
      parent.children.push(
        renderContainer(completed, context, frames.length === 1),
      );
      parent.text += completed.text;
    }
  };
  for (const event of events) {
    const current = frames.at(-1);
    if (current === undefined) break;
    if (event.kind === "start") {
      frames.push({ node: event.node, children: [], text: "", key: key++ });
    } else if (event.kind === "end") {
      if (frames.length > 1) closeFrame();
    } else {
      const leaf = renderLeaf(event, key++);
      if (leaf.node !== null) current.children.push(leaf.node);
      current.text += leaf.text;
    }
  }
  while (frames.length > 1) closeFrame();
  return frames[0]?.children ?? [];
}

function renderLeaf(
  event: SafeMarkdownEvent,
  key: number,
): { node: ReactNode; text: string } {
  switch (event.kind) {
    case "text":
      return { node: event.value, text: event.value };
    case "inlineCode":
      return { node: <code key={key}>{event.value}</code>, text: event.value };
    case "htmlLiteral":
      return {
        node: (
          <code key={key} title="Raw HTML is shown as text">
            {event.value}
          </code>
        ),
        text: event.value,
      };
    case "softBreak":
      return { node: "\n", text: "\n" };
    case "hardBreak":
      return { node: <br key={key} />, text: "\n" };
    case "rule":
      return { node: <hr key={key} />, text: "" };
    case "taskMarker":
      return {
        node: (
          <input
            aria-label={event.checked ? "Done" : "Not done"}
            checked={event.checked}
            className="task-marker mr-1.5 size-3.5 translate-y-0.5 accent-[var(--accent)]"
            key={key}
            readOnly
            type="checkbox"
          />
        ),
        text: event.checked ? "[x] " : "[ ] ",
      };
    default:
      return { node: null, text: "" };
  }
}

function renderContainer(
  frame: Frame,
  context: Context,
  topLevel: boolean,
): ReactNode {
  const { node, children, key, text } = frame;
  if (node === null) return children;
  switch (node.kind) {
    case "paragraph":
      return <p key={key}>{children}</p>;
    case "heading": {
      const id = topLevel ? slugify(text.trim(), context.slugs) : undefined;
      const level = Math.min(6, Math.max(1, node.level));
      const Tag = `h${level}` as "h1";
      return (
        <Tag key={key} id={id}>
          {children}
        </Tag>
      );
    }
    case "blockQuote":
      return <blockquote key={key}>{children}</blockquote>;
    case "codeBlock":
      return <CodeBlock key={key} language={node.language} code={text} />;
    case "list":
      return node.start === null ? (
        <ul key={key}>{children}</ul>
      ) : (
        <ol key={key} start={node.start}>
          {children}
        </ol>
      );
    case "item":
      return <li key={key}>{children}</li>;
    case "emphasis":
      return <em key={key}>{children}</em>;
    case "strong":
      return <strong key={key}>{children}</strong>;
    case "strikethrough":
      return <s key={key}>{children}</s>;
    case "link": {
      const destination = node.destination;
      return destination ? (
        <button
          key={key}
          type="button"
          className="link-button"
          title={destination}
          onClick={() => context.onOpenLink(destination)}
        >
          {children}
        </button>
      ) : (
        <span
          key={key}
          className="text-muted"
          title="This link was blocked because it isn't safe to open."
        >
          {children}
        </span>
      );
    }
    case "image":
      return (
        <span
          key={key}
          className="inline-flex items-center gap-1.5 rounded-md border border-dashed border-border-strong px-2 py-0.5 text-sm text-muted"
          title={node.title ?? "Images in Guides are not loaded."}
        >
          <ImageOff aria-hidden="true" className="size-3.5" />
          {text.trim() || "Image"}
        </span>
      );
    case "table": {
      const head = children.filter(
        (child) => isValidElement(child) && child.type === "thead",
      );
      const rows = children.filter(
        (child) => !(isValidElement(child) && child.type === "thead"),
      );
      return (
        <table key={key}>
          {head}
          <tbody>{rows}</tbody>
        </table>
      );
    }
    case "tableHead":
      return (
        <thead key={key}>
          <tr>{children}</tr>
        </thead>
      );
    case "tableRow":
      return <tr key={key}>{children}</tr>;
    case "tableCell":
      return <td key={key}>{children}</td>;
    case "htmlBlock":
      return (
        <pre
          key={key}
          className="overflow-x-auto rounded-md bg-code p-3 font-mono text-xs text-muted"
        >
          {children}
        </pre>
      );
  }
}

const SHELL_LANGUAGES = new Set([
  "bash",
  "sh",
  "shell",
  "zsh",
  "fish",
  "console",
]);

export function CodeBlock({
  language,
  code,
}: {
  language: string | null;
  code: string;
}) {
  const [copied, setCopied] = useState(false);
  const value = code.replace(/\n$/, "");
  const lines = value.split("\n");
  const shell =
    language !== null && SHELL_LANGUAGES.has(language.toLowerCase());
  // A prompt marks each new command, not comments or continuation lines.
  const prompts = lines.map(
    (line, index) =>
      shell &&
      line.trim() !== "" &&
      !/^\s|^#/.test(line) &&
      !(lines[index - 1] ?? "").trimEnd().endsWith("\\"),
  );

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      toast.success("Copied to clipboard");
      setTimeout(() => setCopied(false), 1400);
    } catch {
      toast.error("Couldn't copy to the clipboard");
    }
  }

  return (
    <div className="group/code relative my-4 overflow-hidden rounded-lg border border-border bg-code">
      <div className="flex h-8 items-center justify-between border-b border-border px-3">
        <span className="font-mono text-2xs text-faint uppercase tracking-wide">
          {language ?? "text"}
        </span>
        <button
          type="button"
          onClick={copy}
          aria-label={copied ? "Copied" : "Copy code"}
          className="-mr-1.5 inline-flex h-6 items-center gap-1 rounded px-1.5 text-xs text-muted hover:bg-hover hover:text-text"
        >
          {copied ? (
            <Check className="size-3.5 text-accent" />
          ) : (
            <Copy className="size-3.5" />
          )}
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre
        className="overflow-x-auto px-4 py-3 font-mono text-[13px] leading-[1.65]"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: wide commands scroll; keyboard users must reach them.
        tabIndex={0}
      >
        <code data-language={language ?? undefined}>
          {lines.map((line, index) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: lines are positional.
            <span key={index} className="block">
              {prompts[index] ? (
                <span
                  aria-hidden="true"
                  className="mr-2 select-none text-faint"
                >
                  $
                </span>
              ) : null}
              {line || "​"}
            </span>
          ))}
        </code>
      </pre>
    </div>
  );
}
