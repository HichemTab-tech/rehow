import type { SafeMarkdownDocument } from "@rehow/contracts";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { extractOutline, MarkdownDocument } from "./MarkdownDocument";

function render(document: SafeMarkdownDocument) {
  return renderToStaticMarkup(
    <MarkdownDocument document={document} onOpenLink={() => undefined} />,
  );
}

describe("safe Guide rendering", () => {
  it("fails closed for an unsupported safe-document version", () => {
    const markup = render({
      version: 2,
      events: [
        { kind: "start", node: { kind: "paragraph" } },
        { kind: "text", value: "hidden" },
        { kind: "end" },
      ],
    });
    expect(markup).toContain('role="alert"');
    expect(markup).not.toContain("hidden");
  });

  it("renders raw HTML blocks and literals as escaped text", () => {
    const markup = render({
      version: 1,
      events: [
        { kind: "start", node: { kind: "htmlBlock" } },
        { kind: "htmlLiteral", value: "<script>alert(1)</script>" },
        { kind: "end" },
        { kind: "start", node: { kind: "paragraph" } },
        { kind: "text", value: "inline " },
        { kind: "htmlLiteral", value: '<img src="file:///tmp/x">' },
        { kind: "end" },
      ],
    });
    expect(markup).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(markup).toContain("&lt;img src=&quot;file:///tmp/x&quot;&gt;");
    expect(markup).not.toContain("<script");
    expect(markup).not.toContain("<img");
  });

  it("blocks navigation and media and exposes code copy controls", () => {
    const markup = render({
      version: 1,
      events: [
        { kind: "start", node: { kind: "paragraph" } },
        {
          kind: "start",
          node: {
            kind: "link",
            destination: "https://example.com/docs",
            title: null,
          },
        },
        { kind: "text", value: "safe link" },
        { kind: "end" },
        {
          kind: "start",
          node: { kind: "link", destination: null, title: null },
        },
        { kind: "text", value: "bad link" },
        { kind: "end" },
        { kind: "start", node: { kind: "image", title: null } },
        { kind: "text", value: "tracker" },
        { kind: "end" },
        { kind: "end" },
        { kind: "start", node: { kind: "codeBlock", language: "sh" } },
        { kind: "text", value: "printf ok\n" },
        { kind: "end" },
      ],
    });
    expect(markup).toMatch(
      /<button[^>]*type="button"[^>]*>safe link<\/button>/,
    );
    expect(markup).toMatch(/<span[^>]*>bad link<\/span>/);
    expect(markup).not.toMatch(/<button[^>]*>bad link/);
    expect(markup).toContain("tracker");
    expect(markup).toContain('aria-label="Copy code"');
    expect(markup).toContain("printf ok");
    expect(markup).not.toContain("<a ");
    expect(markup).not.toContain("href=");
    expect(markup).not.toContain("<img");
    expect(markup).not.toContain("src=");
  });

  it("wraps table body rows in tbody", () => {
    const markup = render({
      version: 1,
      events: [
        { kind: "start", node: { kind: "table", alignments: ["none"] } },
        { kind: "start", node: { kind: "tableHead" } },
        { kind: "start", node: { kind: "tableCell" } },
        { kind: "text", value: "Key" },
        { kind: "end" },
        { kind: "end" },
        { kind: "start", node: { kind: "tableRow" } },
        { kind: "start", node: { kind: "tableCell" } },
        { kind: "text", value: "Value" },
        { kind: "end" },
        { kind: "end" },
        { kind: "end" },
      ],
    });
    expect(markup).toContain(
      "<table><thead><tr><td>Key</td></tr></thead><tbody><tr><td>Value</td></tr></tbody></table>",
    );
  });

  it("assigns heading ids that match the extracted outline", () => {
    const heading = (level: number, text: string, code?: string) => [
      { kind: "start" as const, node: { kind: "heading" as const, level } },
      { kind: "text" as const, value: text },
      ...(code === undefined
        ? []
        : [{ kind: "inlineCode" as const, value: code }]),
      { kind: "end" as const },
    ];
    const document: SafeMarkdownDocument = {
      version: 1,
      events: [
        ...heading(1, "Install"),
        ...heading(2, "Setup"),
        ...heading(2, "Setup"),
        ...heading(3, "Run ", "tar -xf"),
        ...heading(2, "Setup"),
      ],
    };
    const outline = extractOutline(document);
    expect(outline).toEqual([
      { id: "h-install", level: 1, text: "Install" },
      { id: "h-setup", level: 2, text: "Setup" },
      { id: "h-setup-1", level: 2, text: "Setup" },
      { id: "h-run-tar-xf", level: 3, text: "Run tar -xf" },
      { id: "h-setup-2", level: 2, text: "Setup" },
    ]);
    const markup = render(document);
    const ids = [...markup.matchAll(/<h[1-6] id="([^"]+)"/g)].map(
      (match) => match[1],
    );
    expect(ids).toEqual(outline.map((entry) => entry.id));
  });
});

describe("outline ids with inline HTML", () => {
  it("matches rendered heading ids when a heading contains raw HTML", () => {
    const document = {
      version: 1,
      events: [
        { kind: "start", node: { kind: "heading", level: 2 } },
        { kind: "text", value: "Use " },
        { kind: "htmlLiteral", value: "<kbd>" },
        { kind: "text", value: "Ctrl" },
        { kind: "end" },
      ],
    } as const;
    const [entry] = extractOutline(document as never);
    const markup = renderToStaticMarkup(
      <MarkdownDocument
        document={document as never}
        onOpenLink={() => undefined}
      />,
    );
    expect(entry).toBeDefined();
    expect(markup).toContain(`id="${entry?.id}"`);
  });
});
