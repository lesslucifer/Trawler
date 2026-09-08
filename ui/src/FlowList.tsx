import { useCallback, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { defaultProxyAddr, type FlowSummary } from "./api";
import { toRow } from "./flow";
import { NotAnswering } from "./NotAnswering";
import type { FeedFailure } from "./useFlows";
import { DurationText, MethodLabel, Notice, StatusPill, TimestampText } from "./components/index";

const ROW_HEIGHT = 30;

const COLUMNS = "110px 70px 180px 1fr 70px 90px";

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

// The region is empty for four unrelated reasons and none of them may share a
// sentence: telling someone to send traffic they have already sent, or to retry
// a query the core will refuse identically forever, is the failure each separate
// wording exists to prevent.
//
// The order is the precedence rule. A failure outranks both empties, because an
// empty list means nothing when the list could not be fetched — and a refused
// query outranks nothing at all, since it IS the answer to what was asked.
function Absence({
  filtered,
  failure,
  retrying,
  onRetry,
}: {
  filtered: boolean;
  failure: FeedFailure | null;
  retrying: boolean;
  onRetry: () => void;
}) {
  return (
    <div data-testid="empty" className="flex justify-center p-12">
      {failure?.kind === "unreachable" ? (
        <NotAnswering
          trailing="nothing has reached the window"
          retrying={retrying}
          onRetry={onRetry}
        />
      ) : failure?.kind === "rejected" ? (
        // A rejection, not a failure: the API answered and said no, so there is
        // no ⚠ and no retry — the correction is more typing.
        <Notice detail="type a code (404) or a class (4xx)">{`${failure.value} is not a status`}</Notice>
      ) : filtered ? (
        <Notice detail="clear a field to widen the search">no flows match this filter</Notice>
      ) : (
        <Notice detail={`send traffic through the proxy — ${defaultProxyAddr()}`}>no flows yet</Notice>
      )}
    </div>
  );
}

export function FlowList({
  flows,
  selectedId,
  onSelect,
  filtered = false,
  failure = null,
  retrying = false,
  onRetry = () => {},
}: {
  flows: FlowSummary[];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  filtered?: boolean;
  failure?: FeedFailure | null;
  retrying?: boolean;
  onRetry?: () => void;
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
        <Absence filtered={filtered} failure={failure} retrying={retrying} onRetry={onRetry} />
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
