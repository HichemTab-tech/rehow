import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import {
  highlightSelectionMatches,
  search,
  searchKeymap,
} from "@codemirror/search";
import { EditorSelection, EditorState } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  keymap,
  placeholder,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";

const highlight = HighlightStyle.define([
  { tag: tags.heading1, fontWeight: "700", fontSize: "1.25em" },
  { tag: tags.heading2, fontWeight: "700", fontSize: "1.12em" },
  {
    tag: [tags.heading3, tags.heading4, tags.heading5, tags.heading6],
    fontWeight: "650",
  },
  { tag: tags.strong, fontWeight: "700" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: [tags.link, tags.url], color: "var(--accent-text)" },
  { tag: tags.monospace, color: "var(--accent-text)" },
  {
    tag: [tags.processingInstruction, tags.contentSeparator, tags.meta],
    color: "var(--faint)",
  },
  { tag: tags.quote, color: "var(--muted)", fontStyle: "italic" },
  { tag: tags.list, color: "var(--faint)" },
  { tag: [tags.keyword, tags.operatorKeyword], color: "#a24fb3" },
  { tag: [tags.string, tags.special(tags.string)], color: "#2f7d32" },
  { tag: [tags.number, tags.bool], color: "#b2561e" },
  { tag: tags.comment, color: "var(--faint)", fontStyle: "italic" },
  { tag: [tags.variableName, tags.propertyName], color: "var(--info)" },
]);

export type MarkdownEditorHandle = {
  focus: () => void;
  /** Wraps the selection, or inserts the markers around a placeholder. */
  wrap: (before: string, after?: string, placeholderText?: string) => void;
  /** Prefixes every selected line (headings, lists, checklists). */
  prefixLines: (prefix: string) => void;
  insertBlock: (text: string) => void;
};

export const MarkdownEditor = forwardRef<
  MarkdownEditorHandle,
  { value: string; onChange: (value: string) => void; ariaLabel: string }
>(function MarkdownEditor({ value, onChange, ariaLabel }, ref) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // The editor owns its document after mount; external resets remount it via `key`.
  // biome-ignore lint/correctness/useExhaustiveDependencies: create the editor once per mount.
  useEffect(() => {
    if (!host.current) return;
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          history(),
          drawSelection(),
          highlightActiveLine(),
          EditorView.lineWrapping,
          markdown({ base: markdownLanguage, codeLanguages: languages }),
          syntaxHighlighting(highlight),
          search({ top: true }),
          highlightSelectionMatches(),
          placeholder(
            "Write the steps… Use ``` for commands so they can be copied in one click.",
          ),
          keymap.of([
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap,
          ]),
          EditorView.contentAttributes.of({
            "aria-label": ariaLabel,
            spellcheck: "true",
            autocorrect: "off",
          }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged)
              onChangeRef.current(update.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = editor;
    return () => {
      editor.destroy();
      view.current = null;
    };
  }, []);

  useImperativeHandle(ref, () => ({
    focus: () => view.current?.focus(),
    wrap(before, after = before, placeholderText = "text") {
      const editor = view.current;
      if (!editor) return;
      editor.dispatch(
        editor.state.changeByRange((range) => {
          const selected =
            editor.state.sliceDoc(range.from, range.to) || placeholderText;
          const insert = `${before}${selected}${after}`;
          return {
            changes: { from: range.from, to: range.to, insert },
            range: EditorSelection.range(
              range.from + before.length,
              range.from + before.length + selected.length,
            ),
          };
        }),
      );
      editor.focus();
    },
    prefixLines(prefix) {
      const editor = view.current;
      if (!editor) return;
      const { state } = editor;
      const changes = [];
      const seen = new Set<number>();
      for (const range of state.selection.ranges) {
        for (let pos = range.from; pos <= range.to; ) {
          const line = state.doc.lineAt(pos);
          if (!seen.has(line.number)) {
            seen.add(line.number);
            const existing =
              /^(#{1,6} |[-*] \[[ x]\] |[-*] |\d+\. )/.exec(line.text)?.[0] ??
              "";
            changes.push({
              from: line.from,
              to: line.from + existing.length,
              insert: existing === prefix ? "" : prefix,
            });
          }
          pos = line.to + 1;
        }
      }
      editor.dispatch({ changes });
      editor.focus();
    },
    insertBlock(text) {
      const editor = view.current;
      if (!editor) return;
      const { state } = editor;
      const line = state.doc.lineAt(state.selection.main.head);
      const atEmptyLine = line.text.trim() === "";
      const insert = `${atEmptyLine ? "" : "\n\n"}${text}\n`;
      const from = atEmptyLine ? line.from : line.to;
      editor.dispatch({
        changes: { from, to: from, insert },
        selection: { anchor: from + insert.length },
        scrollIntoView: true,
      });
      editor.focus();
    },
  }));

  return <div ref={host} className="h-full min-h-0 overflow-hidden" />;
});
