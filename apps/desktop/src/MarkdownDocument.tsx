import type {
  SafeMarkdownContainer,
  SafeMarkdownDocument,
  SafeMarkdownEvent,
} from "@rehow/contracts";
import type { ReactNode } from "react";

type Frame = {
  node: SafeMarkdownContainer | null;
  children: ReactNode[];
  text: string;
  key: number;
};

export function MarkdownDocument({
  document,
  onAnnouncement,
}: {
  document: SafeMarkdownDocument;
  onAnnouncement: (message: string) => void;
}) {
  if (document.version !== 1) {
    return <p role="alert">This Guide uses an unsupported content version.</p>;
  }
  return (
    <div className="guide-markdown">
      {renderEvents(document.events, onAnnouncement)}
    </div>
  );
}

export function renderEvents(
  events: SafeMarkdownEvent[],
  onAnnouncement: (message: string) => void = () => undefined,
) {
  const frames: Frame[] = [{ node: null, children: [], text: "", key: 0 }];
  let key = 1;
  for (const event of events) {
    const current = frames.at(-1);
    if (current === undefined) break;
    if (event.kind === "start") {
      frames.push({ node: event.node, children: [], text: "", key: key++ });
      continue;
    }
    if (event.kind === "end") {
      if (frames.length === 1) continue;
      const completed = frames.pop();
      const parent = frames.at(-1);
      if (completed?.node !== null && completed !== undefined && parent) {
        parent.children.push(renderContainer(completed, onAnnouncement));
        parent.text += completed.text;
      }
      continue;
    }
    const leaf = renderLeaf(event, key++);
    if (leaf.node !== null) current.children.push(leaf.node);
    current.text += leaf.text;
  }
  while (frames.length > 1) {
    const completed = frames.pop();
    const parent = frames.at(-1);
    if (completed?.node !== null && completed !== undefined && parent) {
      parent.children.push(renderContainer(completed, onAnnouncement));
      parent.text += completed.text;
    }
  }
  return frames[0]?.children ?? [];
}

function renderLeaf(event: SafeMarkdownEvent, key: number) {
  switch (event.kind) {
    case "text":
      return { node: event.value, text: event.value };
    case "inlineCode":
      return { node: <code key={key}>{event.value}</code>, text: event.value };
    case "htmlLiteral":
      return {
        node: (
          <code className="html-literal" key={key}>
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
            aria-label={event.checked ? "Completed" : "Not completed"}
            checked={event.checked}
            className="task-marker"
            key={key}
            readOnly
            type="checkbox"
          />
        ),
        text: event.checked ? "[x] " : "[ ] ",
      };
    case "start":
    case "end":
      return { node: null, text: "" };
  }
}

function renderContainer(
  frame: Frame,
  onAnnouncement: (message: string) => void,
): ReactNode {
  const { node, children, key, text } = frame;
  if (node === null) return children;
  switch (node.kind) {
    case "paragraph":
      return <p key={key}>{children}</p>;
    case "heading":
      return renderHeading(node.level, children, key);
    case "blockQuote":
      return <blockquote key={key}>{children}</blockquote>;
    case "codeBlock":
      return (
        <div className="command-block" key={key}>
          <pre className="command-scroll">
            <code data-language={node.language ?? undefined}>{children}</code>
          </pre>
          <button
            aria-label="Copy code"
            onClick={() => copyCode(text, onAnnouncement)}
            type="button"
          >
            Copy
          </button>
        </div>
      );
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
    case "link":
      return (
        <span
          className={
            node.destination ? "markdown-link" : "markdown-link blocked"
          }
          key={key}
          title={
            node.destination
              ? `${node.destination} — opening external links is not enabled yet`
              : "Unsafe or unsupported link blocked"
          }
        >
          {children}
        </span>
      );
    case "image":
      return (
        <span
          className="image-placeholder"
          key={key}
          title={node.title ?? undefined}
        >
          Image blocked: {children}
        </span>
      );
    case "table":
      return (
        <div className="table-scroll" key={key}>
          <table>{children}</table>
        </div>
      );
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
        <pre className="html-block" key={key}>
          {children}
        </pre>
      );
  }
}

function renderHeading(level: number, children: ReactNode[], key: number) {
  switch (level) {
    case 1:
      return <h2 key={key}>{children}</h2>;
    case 2:
      return <h3 key={key}>{children}</h3>;
    case 3:
      return <h4 key={key}>{children}</h4>;
    case 4:
      return <h5 key={key}>{children}</h5>;
    default:
      return <h6 key={key}>{children}</h6>;
  }
}

async function copyCode(
  text: string,
  onAnnouncement: (message: string) => void,
) {
  try {
    await navigator.clipboard.writeText(text);
    onAnnouncement("Code copied to the clipboard.");
  } catch {
    onAnnouncement("Code could not be copied.");
  }
}
