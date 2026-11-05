import { beforeAll, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { addPairAfter, editPair, findInPairs, parsePairs, removePair } from "../formPairs";
import { FormTable } from "./FormTable";

const ROW = 20;

beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, value: 600 });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 900 });
  HTMLElement.prototype.getBoundingClientRect = () =>
    ({ height: ROW, width: 900, top: 0, left: 0, bottom: ROW, right: 900, x: 0, y: 0, toJSON() {} }) as DOMRect;
});

const FORM = "scope=accounts&scope=sessions%20devices&state=&note=50%ZZ+off";
const noop = () => {};

function table(over: Partial<Parameters<typeof FormTable>[0]> = {}) {
  return render(<FormTable pairs={parsePairs(FORM)} onEdit={noop} onAdd={noop} onRemove={noop} {...over} />);
}

const pairRows = () => screen.getAllByTestId("form-pair");

function Owned({ initial }: { initial: string }) {
  const [text, setText] = useState(initial);
  return (
    <>
      <output data-testid="text">{text}</output>
      <FormTable
        pairs={parsePairs(text)}
        onEdit={(index, part, edited) => setText((t) => editPair(t, index, part, edited))}
        onAdd={(after) => setText((t) => addPairAfter(t, after))}
        onRemove={(index) => setText((t) => removePair(t, index))}
      />
    </>
  );
}

describe("reading", () => {
  it("every pair whole, in wire order, decoded, repeats and bad escapes said in words", () => {
    table();
    const rows = pairRows();
    expect(rows).toHaveLength(4);
    expect(within(rows[1]).getByText("sessions devices")).toHaveAttribute("data-wire", "value");
    expect(rows[0]).toHaveTextContent("1 of 2 “scope”");
    expect(rows[1]).toHaveTextContent("2 of 2 “scope”");
    expect(within(rows[2]).getByText("none")).toHaveClass("text-ink-mute");
    expect(rows[3]).toHaveTextContent("50%ZZ+off");
    expect(rows[3]).toHaveTextContent("kept as sent: not a valid percent-escape");
    expect(screen.getByText("key").parentElement).toHaveClass("sticky");
  });

  it("a long value wraps whole, never clamped", () => {
    const long = "v".repeat(3000);
    table({ pairs: parsePairs(`token=${long}`) });
    const value = within(pairRows()[0]).getByText(long);
    expect(value).toHaveClass("whitespace-pre-wrap", "break-all");
    expect(value.className).not.toMatch(/truncate|line-clamp|nowrap/);
  });

  it("a 300 KiB form is virtualised and its last pair reached whole by scrolling", () => {
    const pairs = Array.from({ length: 6000 }, (_, i) => `k${i}=${"x".repeat(50)}`).join("&");
    expect(pairs.length).toBeGreaterThan(300 * 1024);
    render(<FormTable pairs={parsePairs(pairs)} onEdit={noop} onAdd={noop} onRemove={noop} testId="table" />);
    expect(pairRows().length).toBeLessThan(100);

    const scroller = screen.getByTestId("table");
    act(() => {
      scroller.scrollTop = Number.MAX_SAFE_INTEGER;
      fireEvent.scroll(scroller);
    });
    const last = pairRows()[pairRows().length - 1];
    expect(last).toHaveTextContent("6000");
    expect(within(last).getByText("k5999")).toBeInTheDocument();
    expect(within(last).getByText("x".repeat(50))).toBeInTheDocument();
  });
});

describe("editing in place", () => {
  it("a click opens a verbatim multi-line Field; ⌘↵ commits the one cell", () => {
    render(<Owned initial="a=1&b=2" />);
    fireEvent.click(screen.getByRole("button", { name: "edit the value of pair 2" }));
    const field = screen.getByRole("textbox", { name: "value" });
    expect(field.tagName).toBe("TEXTAREA");
    expect(field).toHaveFocus();
    fireEvent.change(field, { target: { value: "x y&z" } });
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    expect(screen.getByTestId("text")).toHaveTextContent("a=1&b=x%20y%26z");
  });

  it("Esc cancels, even as the Field leaves", () => {
    render(<Owned initial="a=1" />);
    fireEvent.click(screen.getByRole("button", { name: "edit the key of pair 1" }));
    const field = screen.getByRole("textbox", { name: "key" });
    fireEvent.change(field, { target: { value: "changed" } });
    fireEvent.keyDown(field, { key: "Escape" });
    fireEvent.blur(field);
    expect(screen.getByTestId("text")).toHaveTextContent(/^a=1$/);
  });

  it("leaving the Field commits it; an unchanged cell reports nothing", () => {
    const onEdit = vi.fn();
    table({ onEdit });
    fireEvent.keyDown(screen.getByRole("button", { name: "edit the key of pair 1" }), { key: "Enter" });
    fireEvent.blur(screen.getByRole("textbox", { name: "key" }));
    expect(onEdit).not.toHaveBeenCalled();
  });
});

describe("adding and removing", () => {
  it("+ adds a pair after its row and opens the new key's Field", () => {
    render(<Owned initial="a=1&b=2" />);
    fireEvent.click(screen.getByRole("button", { name: "add a pair after pair 1" }));
    expect(screen.getByTestId("text")).toHaveTextContent("a=1&=&b=2");
    expect(screen.getByRole("textbox", { name: "key" })).toHaveFocus();
  });

  it("the new-pair row adds at the end", () => {
    render(<Owned initial="a=1" />);
    fireEvent.keyDown(screen.getByRole("button", { name: "add a pair at the end" }), { key: "Enter" });
    expect(screen.getByTestId("text")).toHaveTextContent("a=1&=");
  });

  it("× removes exactly its pair", () => {
    render(<Owned initial="a=1&a=2&b=3" />);
    fireEvent.click(screen.getByRole("button", { name: "remove pair 2" }));
    expect(screen.getByTestId("text")).toHaveTextContent("a=1&b=3");
  });

  it("the acts show on hover and on the selected row; focus in a row selects it", () => {
    const onSelect = vi.fn();
    table({ selected: 1, onSelect });
    const acts = (row: HTMLElement) => within(row).getByRole("button", { name: /^add a pair after/ }).parentElement!;
    expect(acts(pairRows()[0])).toHaveClass("invisible");
    expect(acts(pairRows()[1])).not.toHaveClass("invisible");
    expect(pairRows()[1]).toHaveClass("bg-canvas-soft");

    fireEvent.mouseEnter(pairRows()[0]);
    expect(acts(pairRows()[0])).not.toHaveClass("invisible");
    fireEvent.mouseLeave(pairRows()[0]);
    expect(acts(pairRows()[0])).toHaveClass("invisible");

    fireEvent.focus(within(pairRows()[2]).getByRole("button", { name: "edit the key of pair 3" }));
    expect(onSelect).toHaveBeenCalledWith(2);
  });
});

describe("the caller's marks, menu and inset", () => {
  it("find marks keys and values, the current one apart", () => {
    table({ matches: findInPairs(parsePairs(FORM), "SCOPE").byPair, current: 1 });
    const marks = document.querySelectorAll("mark");
    expect(marks).toHaveLength(2);
    expect(marks[1]).toHaveClass("bg-wash-active");
    expect(marks[0]).toHaveClass("bg-wash-select");
  });

  it("a right-click selects the row and hands its point to the caller", () => {
    const onContext = vi.fn();
    const onSelect = vi.fn();
    table({ onContext, onSelect });
    fireEvent.contextMenu(pairRows()[2], { clientX: 30, clientY: 40 });
    expect(onSelect).toHaveBeenCalledWith(2);
    expect(onContext).toHaveBeenCalledWith(2, { x: 30, y: 40 });
  });

  it("the inset sits under its row", () => {
    table({ inset: { afterIndex: 0, node: <p>decoded</p> } });
    const first = pairRows()[0];
    expect(first.nextElementSibling).toHaveTextContent("decoded");
  });
});

describe("revealing the current match", () => {
  const long = Array.from({ length: 2000 }, (_, i) => `k${i}=v`).join("&");
  const pairs = parsePairs(long);
  const found = findInPairs(pairs, "k1999").byPair;

  it("scrolls to its row only when the caller bumps reveal", () => {
    const scrollTo = vi.fn();
    HTMLElement.prototype.scrollTo = scrollTo;
    Object.defineProperty(HTMLElement.prototype, "scrollHeight", { configurable: true, value: 10_000_000 });
    const scrolledDown = () => scrollTo.mock.calls.some(([to]) => to.top > 0);
    const props = { pairs, onEdit: noop, onAdd: noop, onRemove: noop, matches: found, current: 0 };
    const { rerender } = render(<FormTable {...props} />);
    expect(scrolledDown()).toBe(false);
    rerender(<FormTable {...props} matches={findInPairs(parsePairs(`x=1&${long}`), "k1999").byPair} />);
    expect(scrolledDown()).toBe(false);
    rerender(<FormTable {...props} reveal={1} />);
    expect(scrolledDown()).toBe(true);
  });
});
