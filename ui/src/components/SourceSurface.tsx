import { useEffect, useRef } from "react";
import { foldGutter, syntaxHighlighting, type LanguageSupport } from "@codemirror/language";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { standardKeymap } from "@codemirror/commands";
import { warpHighlight, warpTheme } from "./warpTheme";

/**
 * SourceSurface — the read-only CodeMirror 6 pane JsonRenderer and XmlRenderer share.
 * @tier T2 — owns the editor configuration that makes a body navigable without being editable: readOnly document, cursor + keyboard motion, fold gutter, and the three-token Warp highlight
 * @anatomy a CodeMirror 6 view holding the source verbatim · fold gutter (▾ / ▸, "…" placeholder) · real cursor and native selection · syntax tokens key / string / number, punctuation muted
 * @use only from JsonRenderer and XmlRenderer — a parsed document earns navigation and folding
 * @avoid a body with no grammar (TextRenderer's plain pre — nothing to navigate, fold or colour) · making it editable · altering the document to highlight or fold it: folding is view state, so selection and copy always yield the exact source
 * @states rendered · folded (view state only) — no empty, loading or error state; the renderer gates on text
 * @madeof CodeMirror 6 (read-only)
 */
export function SourceSurface({
  text,
  language,
  testId,
}: {
  text: string;
  language: LanguageSupport;
  testId: string;
}) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!host.current) return;
    const extensions: Extension[] = [
      language,
      syntaxHighlighting(warpHighlight),
      foldGutter(),
      keymap.of(standardKeymap),
      warpTheme,
      // readOnly, not editable.of(false): the latter drops contenteditable and
      // with it the focusable cursor DEC-13 exists for. readOnly rejects every
      // document-changing transaction while keeping caret motion and selection.
      EditorState.readOnly.of(true),
      EditorView.lineWrapping,
    ];
    const view = new EditorView({
      state: EditorState.create({ doc: text, extensions }),
      parent: host.current,
    });
    return () => view.destroy();
  }, [text, language]);

  return <div ref={host} data-testid={testId} />;
}
