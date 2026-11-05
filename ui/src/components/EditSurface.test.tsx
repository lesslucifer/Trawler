import { describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { EditorSelection } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { foldedRanges } from "@codemirror/language";
import { EditSurface, type EditSurfaceHandle } from "./EditSurface";

const DOC = JSON.stringify({ account: { id: 8812, devices: [{ id: "dv_1" }, { id: "dv_2" }] }, total: 2 }, null, 2);

const viewIn = (container: HTMLElement) => EditorView.findFromDOM(container.querySelector(".cm-editor") as HTMLElement)!;
const folded = (view: EditorView) => {
  let n = 0;
  foldedRanges(view.state).between(0, view.state.doc.length, () => {
    n += 1;
  });
  return n;
};

function type(view: EditorView, at: number, insert: string) {
  act(() => {
    view.dispatch({ changes: { from: at, insert }, userEvent: "input.type" });
  });
}

describe("the document", () => {
  it("holds the whole text, editable, with line numbers and a fold gutter", () => {
    const big = "x".repeat(300_000);
    const { container } = render(<EditSurface text={big} language="plain" testId="surface" />);
    expect(viewIn(container).state.doc.length).toBe(300_000);
    expect(container.querySelector(".cm-content")).toHaveAttribute("contenteditable", "true");
    expect(viewIn(container).state.readOnly).toBe(false);
    expect(container.querySelector(".cm-lineNumbers")).toBeInTheDocument();
    expect(container.querySelector(".cm-foldGutter")).toBeInTheDocument();
    expect(screen.getByTestId("surface")).toHaveAttribute("data-wire", "working copy");
  });

  it("colours json with the three syntax classes only", () => {
    const { container } = render(<EditSurface text={DOC} language="json" />);
    const classes = new Set([...container.querySelectorAll(".cm-content span[class]")].map((s) => s.className));
    expect([...classes].every((c) => ["text-syn-key", "text-syn-str", "text-syn-num", "text-ink-mute"].includes(c))).toBe(true);
    expect(classes.has("text-syn-key")).toBe(true);
  });

  it("colours html and folds its elements", () => {
    const palette = ["text-syn-key", "text-syn-str", "text-syn-num", "text-ink-mute"];
    const { container } = render(<EditSurface text={'<div class="a">\n  <p>hi</p>\n</div>'} language="html" />);
    const classes = [...container.querySelectorAll(".cm-content span[class]")].map((span) => span.className);
    expect(classes.length).toBeGreaterThan(0);
    expect(classes.every((name) => palette.includes(name))).toBe(true);
    expect(container.querySelector(".cm-foldGutter span[title]")).not.toBeNull();
  });

  it("colours javascript and css with the same syntax classes and folds their blocks", () => {
    const palette = ["text-syn-key", "text-syn-str", "text-syn-num", "text-ink-mute"];
    const sources = [
      { language: "javascript", text: 'function f(a) {\n  return "x" + 42 + a;\n}' },
      { language: "css", text: 'a {\n  color: red;\n  margin: 0 4px;\n  content: "x";\n}' },
    ] as const;
    for (const { language, text } of sources) {
      const { container, unmount } = render(<EditSurface text={text} language={language} />);
      const classes = [...container.querySelectorAll(".cm-content span[class]")].map((span) => span.className);
      expect(classes.length).toBeGreaterThan(0);
      expect(classes.every((name) => palette.includes(name))).toBe(true);
      expect(container.querySelector(".cm-foldGutter span[title]")).not.toBeNull();
      unmount();
    }
  });

  it("read-only refuses changes but keeps the cursor", () => {
    const { container } = render(<EditSurface text="abc" language="plain" readOnly />);
    expect(viewIn(container).state.readOnly).toBe(true);
    expect(container.querySelector(".cm-content")).toHaveAttribute("contenteditable", "true");
  });

  it("wraps by default and can stop", () => {
    const { container, rerender } = render(<EditSurface text="abc" language="plain" />);
    const content = () => container.querySelector(".cm-content")!;
    expect(content()).toHaveClass("cm-lineWrapping");
    rerender(<EditSurface text="abc" language="plain" wrap={false} />);
    expect(content()).not.toHaveClass("cm-lineWrapping");
  });
});

describe("changes go to the caller's history", () => {
  it("a user edit reports the text and a named step; the next keystroke in the group extends it", () => {
    const onChange = vi.fn();
    const { container } = render(<EditSurface text={"one\ntwo"} language="plain" onChange={onChange} />);
    const view = viewIn(container);
    type(view, 7, "!");
    type(view, 8, "?");
    expect(onChange.mock.calls).toEqual([
      ["one\ntwo!", "typed on line 2", false],
      ["one\ntwo!?", "typed on line 2", true],
    ]);
  });

  it("reports the text written with the caller's line break, and keeps it on a new line", () => {
    const onChange = vi.fn();
    const { container } = render(<EditSurface text={"a\r\nb"} language="plain" lineBreak={"\r\n"} onChange={onChange} />);
    const view = viewIn(container);
    expect(view.state.doc.lines).toBe(2);
    act(() => {
      view.dispatch(view.state.replaceSelection(view.state.lineBreak));
    });
    expect(onChange.mock.calls[0][0]).toBe("\r\na\r\nb");
  });

  it("text pasted with other breaks is reported with the caller's, so the breaks never mix", () => {
    const onChange = vi.fn();
    const { container } = render(<EditSurface text={"a\r\nb"} language="plain" lineBreak={"\r\n"} onChange={onChange} />);
    const view = viewIn(container);
    act(() => {
      view.dispatch(view.state.replaceSelection("x\ny\rz"));
    });
    expect(onChange.mock.calls[0][0]).toBe("x\r\ny\r\nza\r\nb");
  });

  it("another kind of edit starts a new step", () => {
    const onChange = vi.fn();
    const { container } = render(<EditSurface text="abc" language="plain" onChange={onChange} />);
    const view = viewIn(container);
    type(view, 3, "d");
    act(() => {
      view.dispatch({ changes: { from: 0, to: 1 }, userEvent: "delete.backward" });
    });
    expect(onChange.mock.calls[1]).toEqual(["bcd", "deleted on line 1", false]);
  });

  it("a new text from the caller replaces the document without reporting a change", () => {
    const onChange = vi.fn();
    const { container, rerender } = render(<EditSurface text="abc" language="plain" onChange={onChange} />);
    rerender(<EditSurface text="sorted" language="plain" onChange={onChange} />);
    expect(viewIn(container).state.doc.toString()).toBe("sorted");
    expect(container.querySelectorAll(".cm-editor")).toHaveLength(1);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("an edit after the caller's replace never extends the step before it", () => {
    const onChange = vi.fn();
    const { container, rerender } = render(<EditSurface text="abc" language="plain" onChange={onChange} />);
    type(viewIn(container), 3, "d");
    rerender(<EditSurface text="abcd" language="plain" onChange={onChange} />);
    rerender(<EditSurface text="abc" language="plain" onChange={onChange} />);
    type(viewIn(container), 3, "d");
    expect(onChange.mock.calls.map((call) => call[2])).toEqual([false, false]);
  });
});

describe("selection", () => {
  it("reports the selected text and the line its end is on", () => {
    const onSelect = vi.fn();
    const { container } = render(<EditSurface text={"one\ntwo\nthree"} language="plain" onSelect={onSelect} />);
    act(() => {
      viewIn(container).dispatch({ selection: EditorSelection.single(5, 10) });
    });
    expect(onSelect).toHaveBeenLastCalledWith("wo\nth", 3);
  });
});

describe("find marks", () => {
  it("paints every match, the current one apart", () => {
    const { container } = render(
      <EditSurface text="login logout login" language="plain" matches={[{ from: 0, to: 5 }, { from: 13, to: 18 }]} current={1} />,
    );
    const select = container.querySelectorAll(".cm-content .bg-wash-select");
    const current = container.querySelectorAll(".cm-content .bg-wash-active");
    expect(select).toHaveLength(1);
    expect(current).toHaveLength(1);
    expect(current[0].textContent).toBe("login");
    expect(current[0].className).toContain("outline-ink-mute");
  });

  const hiding = (view: EditorView, at: number) => {
    let hidden = 0;
    foldedRanges(view.state).between(at, at + 4, () => {
      hidden += 1;
    });
    return hidden;
  };

  it("the current match is unfolded when the caller reveals it", () => {
    const ref = createRef<EditSurfaceHandle>();
    const { container, rerender } = render(<EditSurface handle={ref} text={DOC} language="json" />);
    const view = viewIn(container);
    act(() => ref.current!.foldTo("all"));
    expect(folded(view)).toBeGreaterThan(0);
    const at = DOC.indexOf("dv_2");
    const marked = { language: "json" as const, matches: [{ from: at, to: at + 4 }], current: 0 };
    rerender(<EditSurface handle={ref} text={DOC} {...marked} />);
    expect(hiding(view, at)).toBeGreaterThan(0);
    rerender(<EditSurface handle={ref} text={DOC} {...marked} reveal={1} />);
    expect(hiding(view, at)).toBe(0);
  });

  it("an edit that moves the matches reveals nothing", () => {
    const ref = createRef<EditSurfaceHandle>();
    const at = DOC.indexOf("dv_2");
    const { container, rerender } = render(<EditSurface handle={ref} text={DOC} language="json" />);
    const view = viewIn(container);
    act(() => ref.current!.foldTo("all"));
    const edited = ` ${DOC}`;
    type(view, 0, " ");
    rerender(<EditSurface handle={ref} text={edited} language="json" matches={[{ from: at + 1, to: at + 5 }]} current={0} />);
    expect(hiding(view, at + 1)).toBeGreaterThan(0);
  });
});

describe("the handle", () => {
  it("focuses the document, reads the cursor, and maps a point to a position", () => {
    const ref = createRef<EditSurfaceHandle>();
    const { container } = render(<EditSurface handle={ref} text={DOC} language="json" />);
    const view = viewIn(container);
    act(() => ref.current!.focus());
    expect(view.hasFocus).toBe(true);
    act(() => {
      view.dispatch({ selection: EditorSelection.cursor(12) });
    });
    expect(ref.current!.cursor()).toBe(12);
    const spy = vi.spyOn(view, "posAtCoords").mockReturnValue(30);
    expect(ref.current!.positionAt({ x: 4, y: 8 })).toBe(30);
    expect(spy).toHaveBeenCalledWith({ x: 4, y: 8 });
    spy.mockReturnValue(null);
    expect(ref.current!.positionAt({ x: 4, y: 8 })).toBeNull();
  });

  it("reads the selection's range, empty at the cursor", () => {
    const ref = createRef<EditSurfaceHandle>();
    const { container } = render(<EditSurface handle={ref} text={DOC} language="json" />);
    const view = viewIn(container);
    act(() => {
      view.dispatch({ selection: EditorSelection.range(14, 5) });
    });
    expect(ref.current!.selection()).toEqual({ from: 5, to: 14 });
    act(() => {
      view.dispatch({ selection: EditorSelection.cursor(9) });
    });
    expect(ref.current!.selection()).toEqual({ from: 9, to: 9 });
  });

  it("selects a range it is handed", () => {
    const ref = createRef<EditSurfaceHandle>();
    const { container } = render(<EditSurface handle={ref} text={DOC} language="json" />);
    const view = viewIn(container);
    act(() => ref.current!.select({ from: 5, to: 14 }));
    expect(view.state.selection.main).toMatchObject({ from: 5, to: 14 });
  });
});

describe("folding", () => {
  it("folds to a depth, all, and none", () => {
    const ref = createRef<EditSurfaceHandle>();
    const { container } = render(<EditSurface handle={ref} text={DOC} language="json" />);
    const view = viewIn(container);

    act(() => ref.current!.foldTo(1));
    expect(folded(view)).toBe(1);
    act(() => ref.current!.foldTo(2));
    expect(folded(view)).toBe(1);
    act(() => ref.current!.foldTo(3));
    expect(folded(view)).toBe(2);
    act(() => ref.current!.foldTo("none"));
    expect(folded(view)).toBe(0);
    act(() => ref.current!.foldTo("all"));
    expect(folded(view)).toBe(4);
    expect(view.state.doc.lineAt(foldedRanges(view.state).iter().from).number).toBe(2);
  });

  it("a folded line shows how many lines it hides", () => {
    const ref = createRef<EditSurfaceHandle>();
    const { container } = render(<EditSurface handle={ref} text={DOC} language="json" />);
    act(() => ref.current!.foldTo(1));
    expect(container.querySelector(".cm-content .bg-wash-tint")?.textContent).toMatch(/^… \d+ lines$/);
  });
});

describe("the inset", () => {
  it("renders the caller's node as a block under its line, inside the document", () => {
    const { container } = render(
      <EditSurface text={"one\ntwo\nthree"} language="plain" inset={{ afterLine: 2, node: <p>decoded here</p> }} />,
    );
    const node = screen.getByText("decoded here");
    expect(container.querySelector(".cm-content")!.contains(node)).toBe(true);
    const lines = [...container.querySelectorAll(".cm-content > *")];
    const at = lines.findIndex((el) => el.contains(node));
    expect(lines[at - 1].textContent).toBe("two");
  });

  it("is gone once the caller drops it", () => {
    const { rerender } = render(<EditSurface text="one" language="plain" inset={{ afterLine: 1, node: <p>peek</p> }} />);
    rerender(<EditSurface text="one" language="plain" inset={null} />);
    expect(screen.queryByText("peek")).toBeNull();
  });
});
