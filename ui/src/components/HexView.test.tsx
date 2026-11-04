import { beforeAll, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { copiedRange, HexView } from "./HexView";

beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, value: 360 });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 900 });
});

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0x49, 0x45, 0x4e, 0x44]);
const rows = () => screen.getAllByTestId("hex-row");

describe("the dump", () => {
  it("an offset, sixteen bytes and their ASCII per row, the non-printable as a muted dot", () => {
    render(<HexView bytes={PNG} />);
    expect(rows()).toHaveLength(2);
    expect(rows()[0]).toHaveTextContent(/^00000000/);
    expect(rows()[0]).toHaveTextContent("89 50 4e 47".replace(/ /g, ""));
    expect(rows()[1]).toHaveTextContent("00000010");
    expect(rows()[1]).toHaveTextContent("IEND");
    const dot = within(rows()[0]).getAllByText(".")[0];
    expect(dot).toHaveClass("text-ink-mute");
    expect(screen.getByLabelText("the bytes, read-only").querySelector("[data-wire=bytes]")).toHaveClass("font-mono");
  });

  it("a mebibyte is windowed, not drawn whole", () => {
    render(<HexView bytes={new Uint8Array(1024 * 1024)} testId="hex" />);
    expect(rows().length).toBeLessThan(80);
    const scroller = screen.getByTestId("hex");
    act(() => {
      scroller.scrollTop = Number.MAX_SAFE_INTEGER;
      fireEvent.scroll(scroller);
    });
    expect(rows()[rows().length - 1]).toHaveTextContent(/^000ffff0/);
  });
});

describe("find marks", () => {
  it("a match across a row boundary marks bytes on both rows, in both columns", () => {
    render(<HexView bytes={PNG} matches={[{ from: 14, to: 18 }]} current={0} />);
    const marked = document.querySelectorAll(".bg-wash-active");
    expect(marked).toHaveLength(8);
    expect(within(rows()[0]).getAllByText("44")[0]).toHaveClass("bg-wash-active");
    expect(within(rows()[1]).getByText("49")).toHaveClass("bg-wash-active");
  });

  it("matches other than the current one take the lighter wash", () => {
    render(<HexView bytes={PNG} matches={[{ from: 0, to: 1 }, { from: 12, to: 13 }]} current={1} />);
    expect(within(rows()[0]).getByText("89")).toHaveClass("bg-wash-select");
  });
});

describe("selecting and copying a range", () => {
  it("copies from the column the selection started in", () => {
    expect(copiedRange(PNG, { anchor: 12, focus: 15, column: "hex" })).toBe("49 48 44 52");
    expect(copiedRange(PNG, { anchor: 15, focus: 12, column: "ascii" })).toBe("IHDR");
    expect(copiedRange(PNG, { anchor: 0, focus: 1, column: "ascii" })).toBe(".P");
  });

  it("a drag selects; ⌘C writes the range", () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.assign(navigator, { clipboard: { writeText } });
    render(<HexView bytes={PNG} testId="hex" />);
    const byte = (offset: number) => document.querySelector(`[data-offset="${offset}"]`)!;
    fireEvent.mouseDown(byte(12));
    fireEvent.mouseEnter(byte(15));
    fireEvent.mouseUp(window);
    fireEvent.mouseEnter(byte(17));
    expect(byte(15)).toHaveClass("bg-wash-select");
    expect(byte(16)).not.toHaveClass("bg-wash-select");
    fireEvent.keyDown(screen.getByTestId("hex"), { key: "c", metaKey: true });
    expect(writeText).toHaveBeenCalledWith("49 48 44 52");
  });

  it("Shift + arrows extend by a byte, ↑ ↓ by a row", () => {
    render(<HexView bytes={PNG} testId="hex" />);
    const hex = screen.getByTestId("hex");
    const byte = (offset: number) => document.querySelector(`[data-offset="${offset}"]`)!;
    fireEvent.keyDown(hex, { key: "ArrowRight", shiftKey: true });
    fireEvent.keyDown(hex, { key: "ArrowRight", shiftKey: true });
    expect(byte(1)).toHaveClass("bg-wash-select");
    expect(byte(2)).not.toHaveClass("bg-wash-select");
    fireEvent.keyDown(hex, { key: "ArrowDown", shiftKey: true });
    expect(byte(17)).toHaveClass("bg-wash-select");
  });
});
