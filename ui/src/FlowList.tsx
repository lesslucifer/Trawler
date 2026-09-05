import { useCallback, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { FlowSummary } from "./api";
import { toRow } from "./flow";
import { DurationText, MethodLabel, Notice, StatusPill, TimestampText } from "./components/index";

const ROW_HEIGHT = 30;

const COLUMNS = "110px 70px 180px 1fr 70px 90px";

const PROXY_ADDR = "127.0.0.1:7420";

function HeaderRow() {
  return (
    <div
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

// The list is empty for two unrelated reasons and they must not share a
// sentence: telling someone to send traffic they have already sent is the
// failure the second message exists to prevent.
function Absence({ filtered }: { filtered: boolean }) {
  return (
    <div data-testid="empty" className="flex justify-center p-12">
      {filtered ? (
        <Notice detail="clear a field to widen the search">no flows match this filter</Notice>
      ) : (
        <Notice detail={`send traffic through the proxy — ${PROXY_ADDR}`}>no flows yet</Notice>
      )}
    </div>
  );
}

export function FlowList({
  flows,
  selectedId,
  onSelect,
  filtered = false,
}: {
  flows: FlowSummary[];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  filtered?: boolean;
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
        <Absence filtered={filtered} />
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
                <TimestampText timestamp={flow.started_at} />
                <MethodLabel method={flow.method} />
                <span data-wire="host" className="truncate">
                  {row.host}
                </span>
                <span data-wire="path" className="truncate">
                  {row.path}
                </span>
                <span className="text-center">
                  <StatusPill status={flow.status} />
                </span>
                <DurationText value={flow.duration_ms} />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
