import { useCallback, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { defaultProxyAddr, type FlowSummary } from "./api";
import { toRow } from "./flow";
import { DurationText, MethodLabel, Notice, StatusPill, TimestampText } from "./components/index";

const ROW_HEIGHT = 30;

// Cell 1 is sized by the timestamp import produces, not by the mark it also
// holds: a HAR handed over from another machine was captured on another day, so
// TimestampText's cross-day form (115.2px) plus the mark's reserved 16px box is
// what the column has to hold. The mark itself never enters the arithmetic.
const COLUMNS = "160px 70px 180px 1fr 70px 90px";

function HeaderRow() {
  return (
    <div
      data-wire="none"
      className="grid gap-3 border-b border-hairline px-4 py-chrome-y text-body uppercase tracking-label text-ink-mute"
      style={{ gridTemplateColumns: COLUMNS }}
    >
      <span>Time</span>
      <span>Method</span>
      <span>Host</span>
      <span>Path</span>
      <span className="text-center">Status</span>
      <span className="text-right">Duration</span>
    </div>
  );
}

// The list is empty for three unrelated reasons and they must not share a
// sentence: telling someone to send traffic they have already sent is the
// failure the second message exists to prevent — and a user who just cleared the
// capture is exactly that person, which is why clearing gets a heading of its
// own rather than landing on the never-captured one. The remedy is shared
// because after a clear it genuinely is the same remedy.
function Absence({ filtered, cleared }: { filtered: boolean; cleared: boolean }) {
  const refill = `send traffic through the proxy — ${defaultProxyAddr()}`;
  return (
    <div data-testid="empty" className="flex justify-center p-12">
      {cleared ? (
        <Notice detail={refill}>capture cleared</Notice>
      ) : filtered ? (
        <Notice detail="clear a field to widen the search">no flows match this filter</Notice>
      ) : (
        <Notice detail={refill}>no flows yet</Notice>
      )}
    </div>
  );
}

// The mark wears the door's own face — the same ↑ the import control puts in the
// row — and its box is reserved whether or not it is drawn, so every timestamp
// shares one left edge. Not a column and not a component: one conditional
// character with an accessible name, in one cell of one screen.
function ImportMark({ imported }: { imported: boolean }) {
  return (
    <span
      aria-label={imported ? "imported" : undefined}
      title={imported ? "imported" : undefined}
      className="inline-block w-4 text-ink-mute"
    >
      {imported ? "↑" : ""}
    </span>
  );
}

export function FlowList({
  flows,
  selectedId,
  onSelect,
  filtered = false,
  cleared = false,
}: {
  flows: FlowSummary[];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  filtered?: boolean;
  cleared?: boolean;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: flows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  const selectedIndex = flows.findIndex((flow) => flow.id === selectedId);

  // A windowed list only renders its own window, so moving the selection with
  // the keys past that window would otherwise select a row with no element to
  // scroll to — the keyboard path would be nominal.
  const { scrollToIndex } = virtualizer;
  const move = useCallback(
    (delta: number) => {
      if (flows.length === 0) return;
      const next = selectedIndex < 0 ? 0 : selectedIndex + delta;
      if (next < 0 || next >= flows.length) return;
      scrollToIndex(next);
      onSelect?.(flows[next].id);
    },
    [flows, selectedIndex, onSelect, scrollToIndex],
  );

  useEffect(() => {
    if (selectedIndex >= 0) scrollToIndex(selectedIndex);
  }, [selectedIndex, scrollToIndex]);

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (selectedIndex < 0) move(1);
      else onSelect?.(flows[selectedIndex].id);
    }
  };

  if (flows.length === 0) {
    return (
      <div className="flex h-full flex-col">
        <HeaderRow />
        <Absence filtered={filtered} cleared={cleared} />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <HeaderRow />
      <div
        ref={parentRef}
        data-testid="flow-scroll"
        role="listbox"
        aria-label="flows"
        data-wire="collection"
        data-order="wire"
        aria-activedescendant={selectedIndex < 0 ? undefined : `flow-row-${selectedId}`}
        tabIndex={0}
        onKeyDown={onKeyDown}
        className="min-h-0 flex-1 overflow-auto"
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const flow = flows[item.index];
            const row = toRow(flow);
            const selected = flow.id === selectedId;
            return (
              <div
                key={flow.id}
                id={`flow-row-${flow.id}`}
                data-testid="flow-row"
                role="option"
                aria-selected={selected}
                onClick={() => onSelect?.(flow.id)}
                title={row.path}
                className={`absolute left-0 top-0 grid w-full cursor-pointer items-center gap-3 border-b border-hairline px-4 font-mono text-ink-body hover:bg-canvas-soft ${
                  selected ? "bg-canvas-soft" : ""
                }`}
                style={{
                  gridTemplateColumns: COLUMNS,
                  height: ROW_HEIGHT,
                  transform: `translateY(${item.start}px)`,
                }}
              >
                <span data-wire="started_at">
                  <ImportMark imported={flow.source === "imported"} />
                  <TimestampText timestamp={flow.started_at} />
                </span>
                <span data-wire="method">
                  <MethodLabel method={flow.method} />
                </span>
                <span data-wire="host" className="truncate">
                  {row.host}
                </span>
                <span data-wire="path" className="truncate">
                  {row.path}
                </span>
                <span data-wire="status" className="text-center">
                  <StatusPill status={flow.status} />
                </span>
                <span data-wire="duration_ms">
                  <DurationText value={flow.duration_ms} />
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
