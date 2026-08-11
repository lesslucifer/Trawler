import { describe, expect, it } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import { json } from "@codemirror/lang-json";
import { cursorCharRight, cursorLineEnd, deleteCharBackward } from "@codemirror/commands";
import { EditorView } from "@codemirror/view";
import { SourceSurface } from "./SourceSurface";

const language = json();
const DOC = '{\n  "name": "Alice",\n  "count": 42\n}';

const viewIn = (host: HTMLElement) => {
  const dom = host.querySelector<HTMLElement>(".cm-editor");
  return dom ? EditorView.findFromDOM(dom) : null;
};

describe("SourceSurface", () => {
  it("holds the text verbatim as its document", () => {
    const { container } = render(<SourceSurface text={DOC} language={language} testId="surface" />);
    expect(viewIn(container)?.state.doc.toString()).toBe(DOC);
  });

  // Two independent guards, because readOnly alone is a facet — the editing
  // commands that consult it refuse, but nothing vetoes an arbitrary dispatch.
  // The surface therefore also binds a navigation-only keymap: no editing
  // command is reachable from the keyboard in the first place.
  it("refuses user editing — a body pane is navigable, never editable", () => {
    const { container } = render(<SourceSurface text={DOC} language={language} testId="surface" />);
    const view = viewIn(container);

    expect(view?.state.readOnly).toBe(true);
    expect(deleteCharBackward(view!)).toBe(false);
    expect(view?.state.doc.toString()).toBe(DOC);
  });

  it("binds no editing command to the keyboard — the keymap is navigation only", () => {
    const { container } = render(<SourceSurface text={DOC} language={language} testId="surface" />);
    const content = container.querySelector(".cm-content")!;

    fireEvent.keyDown(content, { key: "Enter" });
    fireEvent.keyDown(content, { key: "Backspace" });
    fireEvent.keyDown(content, { key: "a" });

    expect(viewIn(container)?.state.doc.toString()).toBe(DOC);
  });

  // Vertical motion needs real line geometry, which jsdom has none of; the
  // line-boundary commands exercise the same keymap without it.
  it("navigates by keyboard — the cursor DEC-13 asked for actually moves", () => {
    const { container } = render(<SourceSurface text={DOC} language={language} testId="surface" />);
    const view = viewIn(container)!;

    view.dispatch({ selection: { anchor: 0 } });
    cursorLineEnd(view);
    expect(view.state.selection.main.head).toBe(1);

    cursorCharRight(view);
    expect(view.state.selection.main.head).toBe(2);

    expect(view.state.doc.toString()).toBe(DOC);
  });

  it("keeps the caret: contenteditable stays, which is what carries keyboard motion", () => {
    const { container } = render(<SourceSurface text={DOC} language={language} testId="surface" />);
    expect(container.querySelector(".cm-content")).toHaveAttribute("contenteditable", "true");
  });

  // Palette tokens by class, never a hex value: a colour written here is a
  // second copy of the token, and no class-reading check can see it — which is
  // how these three survived the config deletion that was meant to remove them.
  it("colours only the three DEC-11 tokens — key, string, number — and does it with palette classes", () => {
    const { container } = render(<SourceSurface text={DOC} language={language} testId="surface" />);
    const tokens = [...container.querySelectorAll<HTMLElement>(".cm-content span[class]")];
    const classes = new Set(tokens.map((span) => span.className));

    expect(classes).toEqual(new Set(["text-syn-key", "text-syn-str", "text-syn-num", "text-ink-mute"]));
    expect(container.querySelectorAll(".cm-content span[style*=color]")).toHaveLength(0);
  });

  it("offers the fold gutter", () => {
    const { container } = render(<SourceSurface text={DOC} language={language} testId="surface" />);
    expect(container.querySelector(".cm-foldGutter")).toBeInTheDocument();
  });

  it("swaps the document when the body changes, rather than appending a second view", () => {
    const { container, rerender } = render(
      <SourceSurface text={DOC} language={language} testId="surface" />,
    );
    rerender(<SourceSurface text='{"other":1}' language={language} testId="surface" />);

    expect(container.querySelectorAll(".cm-editor")).toHaveLength(1);
    expect(viewIn(container)?.state.doc.toString()).toBe('{"other":1}');
  });
});
