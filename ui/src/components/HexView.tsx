import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { writeClipboard } from "./CopyButton";
import type { TextRange } from "./EditSurface";

const WIDTH = 16;
const ROW_HEIGHT = 18;
const NO_MATCHES: TextRange[] = [];

type Column = "hex" | "ascii";

interface Selection {
  anchor: number;
  focus: number;
  column: Column;
}

const hexOf = (byte: number) => byte.toString(16).padStart(2, "0");
const charOf = (byte: number) => (byte >= 0x20 && byte < 0x7f ? String.fromCharCode(byte) : ".");

function matchAt(matches: TextRange[], offset: number): number {
  let low = 0;
  let high = matches.length - 1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (matches[mid].to <= offset) low = mid + 1;
    else if (matches[mid].from > offset) high = mid - 1;
    else return mid;
  }
  return -1;
}

export function copiedRange(bytes: Uint8Array, selection: Selection): string {
  const from = Math.min(selection.anchor, selection.focus);
  const to = Math.max(selection.anchor, selection.focus) + 1;
  const range = Array.from(bytes.subarray(from, to));
  return selection.column === "hex" ? range.map(hexOf).join(" ") : range.map(charOf).join("");
}

/**
 * HexView — the whole hex dump of a payload in the data window, virtualised and read-only, with byte-range find marks and a copyable selection.
 * @tier T2 — owns the dump layout (offset · 16 bytes · ASCII) and the range selection; the bytes and the match ranges are the caller's
 * @anatomy offset ink-mute · 16 bytes ink-body, wider gap after the 8th · ASCII, non-printable as ink-mute "." · matched bytes wash-select + ink in both columns, current wash-active · selection wash-select in both columns
 * @use the data window's hex surface (DEC-69): a binary payload, or any payload the user views as hex
 * @avoid the body pane (HexRenderer, capped at HEX_MAX by contract) · editing bytes · a cut marker or any cap · one text block for the whole dump: a row per element with bytes as spans is what lets a mark sit on a byte and the list window
 * @states rest · with find marks · with a selected range. No editable, empty or loading state
 * @madeof @tanstack/react-virtual (T0)
 */
export function HexView({
  bytes,
  matches = NO_MATCHES,
  current = -1,
  onCopy = (text) => void writeClipboard(text),
  testId,
}: {
  bytes: Uint8Array;
  matches?: TextRange[];
  current?: number;
  onCopy?: (text: string) => void;
  testId?: string;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const dragging = useRef(false);
  const rows = Math.ceil(bytes.length / WIDTH);

  const virtualizer = useVirtualizer({
    count: rows,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 16,
  });

  useEffect(() => {
    const target = matches[current];
    if (target) virtualizer.scrollToIndex(Math.floor(target.from / WIDTH), { align: "center" });
  }, [matches, current, virtualizer]);

  useEffect(() => {
    const release = () => {
      dragging.current = false;
    };
    window.addEventListener("mouseup", release);
    return () => window.removeEventListener("mouseup", release);
  }, []);

  const selected = (offset: number) =>
    selection !== null &&
    offset >= Math.min(selection.anchor, selection.focus) &&
    offset <= Math.max(selection.anchor, selection.focus);

  const tone = (offset: number, rest: string) => {
    const hit = matchAt(matches, offset);
    if (hit >= 0) return hit === current ? "bg-wash-active text-ink" : "bg-wash-select text-ink";
    return selected(offset) ? "bg-wash-select text-ink" : rest;
  };

  const press = (offset: number, column: Column) => (event: MouseEvent) => {
    event.preventDefault();
    dragging.current = true;
    scroller.current?.focus({ preventScroll: true });
    setSelection(event.shiftKey && selection ? { ...selection, focus: offset } : { anchor: offset, focus: offset, column });
  };

  const drag = (offset: number) => () => {
    if (dragging.current) setSelection((s) => (s ? { ...s, focus: offset } : s));
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c" && selection) {
      event.preventDefault();
      onCopy(copiedRange(bytes, selection));
      return;
    }
    const by = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -WIDTH, ArrowDown: WIDTH }[event.key];
    if (!event.shiftKey || by === undefined || bytes.length === 0) return;
    event.preventDefault();
    const start = selection ?? { anchor: 0, focus: 0, column: "hex" as Column };
    const focus = Math.max(0, Math.min(bytes.length - 1, selection ? start.focus + by : 0));
    setSelection({ ...start, focus });
    virtualizer.scrollToIndex(Math.floor(focus / WIDTH));
  };

  return (
    <div
      ref={scroller}
      data-testid={testId}
      tabIndex={0}
      aria-label="the bytes, read-only"
      onKeyDown={onKeyDown}
      className="min-h-0 flex-1 overflow-auto outline-none"
    >
      <div data-wire="bytes" className="relative px-4 py-2 font-mono text-body leading-normal" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((item) => {
          const start = item.index * WIDTH;
          const chunk = Array.from(bytes.subarray(start, start + WIDTH));
          return (
            <div
              key={item.key}
              data-testid="hex-row"
              className="absolute left-4 top-0 flex gap-4 select-none"
              style={{ height: ROW_HEIGHT, transform: `translateY(${item.start}px)` }}
            >
              <span className="text-ink-mute">{start.toString(16).padStart(8, "0")}</span>
              <span className="flex gap-chrome-gap">
                {chunk.map((byte, j) => (
                  <span
                    key={j}
                    data-offset={start + j}
                    onMouseDown={press(start + j, "hex")}
                    onMouseEnter={drag(start + j)}
                    className={`rounded-button ${j === 8 ? "ml-2" : ""} ${tone(start + j, "text-ink-body")}`}
                  >
                    {hexOf(byte)}
                  </span>
                ))}
              </span>
              <span className="whitespace-pre">
                {chunk.map((byte, j) => (
                  <span
                    key={j}
                    onMouseDown={press(start + j, "ascii")}
                    onMouseEnter={drag(start + j)}
                    className={tone(start + j, byte >= 0x20 && byte < 0x7f ? "text-ink-body" : "text-ink-mute")}
                  >
                    {charOf(byte)}
                  </span>
                ))}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
