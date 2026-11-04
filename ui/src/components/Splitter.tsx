import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

const STEP_PX = 8;
const LARGE_STEP_PX = 48;

const AXES = {
  x: {
    forward: "ArrowRight",
    back: "ArrowLeft",
    orientation: "vertical",
    cursor: "cursor-col-resize",
    line: "w-px",
    band: "inset-y-0 -inset-x-grip-reach",
    along: (e: PointerEvent) => e.clientX,
  },
  y: {
    forward: "ArrowDown",
    back: "ArrowUp",
    orientation: "horizontal",
    cursor: "cursor-row-resize",
    line: "h-px",
    band: "inset-x-0 -inset-y-grip-reach",
    along: (e: PointerEvent) => e.clientY,
  },
} as const;

/**
 * Splitter — a line between two zones the user drags or steps to size one of them between two bounds.
 * @tier T1 — position in, clamped value out; which value, its bounds and where it is saved are the caller's
 * @anatomy 1px line (hairline at rest, ink-mute when active) · transparent hit band grip-reach past each side,
 *   z-sticky, resize cursor · the shared focus ring, inset
 * @use any split the user sizes: list ↔ pane, headers ↔ body, a fixed column's boundary in a label row
 * @avoid a split that collapses or hides a zone (none does) · a fixed layout boundary, which stays a border
 *   · choosing or saving the value inside it · a visible grip glyph or gutter · a value with known options,
 *   which is a SegmentedControl or a Select
 * @states rest · hover · dragging (document-wide resize cursor, no text selection) · focus (keyboard only).
 *   No disabled, collapsed or at-bound state
 * @madeof —
 */
export function Splitter({
  axis,
  value,
  min,
  max,
  dir,
  perPx = 1,
  onChange,
  onCommit,
  onReset,
  label,
  valueText,
  className,
}: {
  axis: "x" | "y";
  value: number;
  min: number;
  max: number;
  dir: 1 | -1;
  perPx?: number;
  onChange: (value: number) => void;
  onCommit: () => void;
  onReset: () => void;
  label: string;
  valueText: string;
  className?: string;
}) {
  const spec = AXES[axis];
  const within = (v: number) => Math.min(Math.max(v, min), max);
  const shown = within(value);
  const drag = useRef<{ origin: number; start: number; moved: boolean } | null>(null);
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    if (!dragging) return;
    const root = document.documentElement.classList;
    const marks = [spec.cursor, "select-none"];
    root.add(...marks);
    return () => root.remove(...marks);
  }, [dragging, spec.cursor]);

  const press = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    drag.current = { origin: spec.along(e), start: shown, moved: false };
    setDragging(true);
  };

  const move = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    d.moved = true;
    onChange(within(d.start + dir * (spec.along(e) - d.origin) * perPx));
  };

  const release = () => {
    const d = drag.current;
    if (!d) return;
    drag.current = null;
    setDragging(false);
    if (d.moved) onCommit();
  };

  const step = (e: KeyboardEvent<HTMLDivElement>) => {
    const px = e.shiftKey ? LARGE_STEP_PX : STEP_PX;
    const next =
      e.key === spec.forward
        ? within(shown + dir * px * perPx)
        : e.key === spec.back
          ? within(shown - dir * px * perPx)
          : e.key === "Home"
            ? min
            : e.key === "End"
              ? max
              : null;
    if (next === null) return;
    e.preventDefault();
    onChange(next);
    onCommit();
  };

  return (
    <div
      data-testid="splitter-line"
      className={`relative shrink-0 ${spec.line} ${dragging ? "bg-ink-mute" : "bg-hairline hover:bg-ink-mute"}`}
    >
      <div
        role="separator"
        tabIndex={0}
        aria-orientation={spec.orientation}
        aria-label={label}
        aria-valuenow={shown}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuetext={valueText}
        className={`absolute z-sticky ${className ?? spec.band} ${spec.cursor} focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute focus-visible:outline-offset-focus-inset`}
        onPointerDown={press}
        onPointerMove={move}
        onPointerUp={release}
        onPointerCancel={release}
        onKeyDown={step}
        onDoubleClick={() => {
          onReset();
          onCommit();
        }}
      />
    </div>
  );
}
