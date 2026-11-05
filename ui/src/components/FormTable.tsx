import { Fragment, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { repeats, type FormPair, type PairMatches, type PairPart } from "../formPairs";
import type { TextRange } from "./EditSurface";
import { Field } from "./Field";
import { IconButton } from "./IconButton";

export interface RowInset {
  afterIndex: number;
  node: ReactNode;
}

interface Editing {
  index: number;
  part: PairPart;
  draft: string;
}

const COLUMNS = { gridTemplateColumns: "5ch minmax(128px, 2fr) 5fr 60px" };
const MARK = "rounded-button bg-wash-select text-ink";
const CURRENT = "rounded-button bg-wash-active text-ink outline outline-focus outline-ink-mute";

const NO_MATCHES = new Map<number, PairMatches>();

function Marked({ text, ranges, first, current }: { text: string; ranges: TextRange[]; first: number; current: number }) {
  if (!ranges.length) return <>{text}</>;
  const parts: ReactNode[] = [];
  let from = 0;
  ranges.forEach((range, n) => {
    parts.push(text.slice(from, range.from));
    parts.push(
      <mark key={range.from} data-match={first + n} className={first + n === current ? CURRENT : MARK}>
        {text.slice(range.from, range.to)}
      </mark>,
    );
    from = range.to;
  });
  parts.push(text.slice(from));
  return <>{parts}</>;
}

function rowOf(matches: Map<number, PairMatches>, current: number): number | null {
  for (const [index, found] of matches) {
    if (current >= found.first && current < found.first + found.key.length + found.value.length) return index;
  }
  return null;
}

/**
 * FormTable — every pair of a URL-encoded form, uncapped, each key and value whole and wrapped, edited in place.
 * @tier T2 — owns display decoding, the duplicate-key count and the never-cut rule; the pairs, their order and what an edit means are the caller's
 * @anatomy sticky uppercase-mute label row (# · key · value) · number ink-mute · key mono syn-key, "N of M “key”" under a repeat · value mono ink, wrapped whole, "none" when empty, "kept as sent" under a bad escape · IconButton add + and close × per row, invisible until the row is hovered or selected — focus in a row selects it · selected row canvas-soft · the edited cell a multiline verbatim Field · a new-pair row (new key · new value, ink-mute) after the last · an inset block under one row
 * @use the data window's form surface, decoded (DEC-75)
 * @avoid the body pane (FormRenderer, capped and read-only) · truncating, clamping or fixing a row's height (DEC-34's escape is not taken here: rows are measured, so a 3 KiB value wraps whole) · re-encoding pairs nobody edited · sorting or finding inside it: the caller hands the order and the marks · hiding a row's acts from the keyboard: focus shows them as hover does · a Field in every cell: one cell is edited at a time, the rest stay a reading surface · KeyValueRow's clamped, read-only rows
 * @states rest · row hovered or focused (acts shown) · selected row · editing one cell · with find marks (the caller's, per pair; the current one's row scrolled to only when the caller bumps reveal, so an edit that moves the marks leaves the view where the user is) · with an inset under one row. No empty, collapsed or loading state
 * @madeof Field (T1) — the edited cell · IconButton (T1) — add and remove · @tanstack/react-virtual (T0)
 */
export function FormTable({
  pairs,
  selected = null,
  onSelect,
  onEdit,
  onAdd,
  onRemove,
  onContext,
  inset = null,
  matches = NO_MATCHES,
  current = -1,
  reveal = 0,
  testId,
}: {
  pairs: FormPair[];
  selected?: number | null;
  onSelect?: (index: number) => void;
  onEdit: (index: number, part: PairPart, text: string) => void;
  onAdd: (afterIndex: number) => void;
  onRemove: (index: number) => void;
  onContext?: (index: number, point: { x: number; y: number }) => void;
  inset?: RowInset | null;
  matches?: Map<number, PairMatches>;
  current?: number;
  reveal?: number;
  testId?: string;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const editor = useRef<HTMLDivElement>(null);
  const [editing, setEditingState] = useState<Editing | null>(null);
  const live = useRef<Editing | null>(null);
  const setEditing = (next: Editing | null) => {
    live.current = next;
    setEditingState(next);
  };
  const opening = useRef<number | null>(null);
  const [hovered, setHovered] = useState<number | null>(null);
  const counted = useMemo(() => repeats(pairs), [pairs]);

  const virtualizer = useVirtualizer({
    count: pairs.length + 1,
    getScrollElement: () => scroller.current,
    estimateSize: () => 28,
    overscan: 8,
  });

  const shownMarks = useRef({ matches, current });
  shownMarks.current = { matches, current };
  useEffect(() => {
    const row = rowOf(shownMarks.current.matches, shownMarks.current.current);
    if (reveal && row !== null) virtualizer.scrollToIndex(row, { align: "center" });
  }, [reveal, virtualizer]);

  useEffect(() => {
    if (opening.current === null || !pairs[opening.current]) return;
    setEditing({ index: opening.current, part: "key", draft: pairs[opening.current].key });
    opening.current = null;
  }, [pairs]);

  useEffect(() => {
    editor.current?.querySelector("textarea")?.focus();
  }, [editing?.index, editing?.part]);

  const add = (after: number) => {
    opening.current = after + 1;
    onAdd(after);
  };

  const commit = () => {
    const done = live.current;
    if (!done) return;
    setEditing(null);
    const pair = pairs[done.index];
    if (pair && pair[done.part] !== done.draft) onEdit(done.index, done.part, done.draft);
  };

  const open = (pair: FormPair, part: PairPart) => {
    onSelect?.(pair.index);
    setEditing({ index: pair.index, part, draft: pair[part] });
  };

  const editorKeys = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setEditing(null);
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      commit();
    }
  };

  const cell = (pair: FormPair, part: PairPart) => {
    if (editing && editing.index === pair.index && editing.part === part) {
      return (
        <div ref={editor} onKeyDown={editorKeys} onBlur={commit}>
          <Field
            value={editing.draft}
            onChange={(event) => setEditing({ ...editing, draft: event.target.value })}
            aria-label={part}
            multiline
            verbatim
            className="h-12 w-full"
          />
        </div>
      );
    }
    const text = pair[part];
    const found = matches.get(pair.index);
    const first = found ? found.first + (part === "value" ? found.key.length : 0) : 0;
    return (
      <div
        data-wire={part}
        role="button"
        tabIndex={0}
        aria-label={`edit the ${part} of pair ${pair.index + 1}`}
        onClick={() => open(pair, part)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            open(pair, part);
          }
        }}
        className={`cursor-text whitespace-pre-wrap break-all rounded-button font-mono text-body leading-normal hover:bg-wash-hover ${
          part === "key" ? "text-syn-key" : text ? "text-ink" : "text-ink-mute"
        }`}
      >
        {text ? <Marked text={text} ranges={found?.[part] ?? []} first={first} current={current} /> : "none"}
      </div>
    );
  };

  const row = (pair: FormPair) => {
    const repeat = counted.get(pair.index);
    const chosen = selected === pair.index;
    const showActs = chosen || hovered === pair.index;
    return (
      <Fragment>
        <div
          data-testid="form-pair"
          onFocus={() => onSelect?.(pair.index)}
          onMouseEnter={() => setHovered(pair.index)}
          onMouseLeave={() => setHovered((h) => (h === pair.index ? null : h))}
          onClick={() => onSelect?.(pair.index)}
          onContextMenu={(event) => {
            event.preventDefault();
            onSelect?.(pair.index);
            onContext?.(pair.index, { x: event.clientX, y: event.clientY });
          }}
          style={COLUMNS}
          className={`grid items-start gap-3 border-b border-hairline px-4 py-chrome-y ${chosen ? "bg-canvas-soft" : ""}`}
        >
          <span className="pt-chrome-tight text-right font-ui text-chrome text-ink-mute">{pair.index + 1}</span>
          <div>
            {cell(pair, "key")}
            {repeat && (
              <span className="font-ui text-chrome text-ink-mute">
                {repeat.nth} of {repeat.of} “{pair.key}”
              </span>
            )}
          </div>
          <div>
            {cell(pair, "value")}
            {pair.kept && (
              <span className="font-ui text-chrome text-ink-mute">kept as sent: not a valid percent-escape</span>
            )}
          </div>
          <span className={`flex ${showActs ? "" : "invisible"}`}>
            <IconButton glyph="add" label={`add a pair after pair ${pair.index + 1}`} onClick={() => add(pair.index)} />
            <IconButton glyph="close" label={`remove pair ${pair.index + 1}`} onClick={() => onRemove(pair.index)} />
          </span>
        </div>
        {inset && inset.afterIndex === pair.index && (
          <div className="border-b border-hairline py-chrome-y pl-12 pr-4">{inset.node}</div>
        )}
      </Fragment>
    );
  };

  const newPair = (
    <div
      role="button"
      tabIndex={0}
      aria-label="add a pair at the end"
      onClick={() => add(pairs.length - 1)}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          add(pairs.length - 1);
        }
      }}
      style={COLUMNS}
      className="grid cursor-text gap-3 px-4 py-chrome-y font-ui text-chrome text-ink-mute hover:bg-wash-hover"
    >
      <span />
      <span>new key</span>
      <span>new value</span>
      <span />
    </div>
  );

  return (
    <div ref={scroller} data-testid={testId} className="min-h-0 flex-1 overflow-auto">
      <div
        style={COLUMNS}
        className="sticky top-0 z-sticky grid gap-3 border-b border-hairline bg-canvas px-4 py-chrome-y text-label uppercase tracking-label text-ink-mute"
      >
        <span>#</span>
        <span>key</span>
        <span>value</span>
        <span />
      </div>
      <div data-wire="collection" data-order="wire" style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {virtualizer.getVirtualItems().map((item) => (
          <div
            key={item.key}
            data-index={item.index}
            ref={virtualizer.measureElement}
            className="absolute left-0 top-0 w-full"
            style={{ transform: `translateY(${item.start}px)` }}
          >
            {item.index < pairs.length ? row(pairs[item.index]) : newPair}
          </div>
        ))}
      </div>
    </div>
  );
}
