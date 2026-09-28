import { useCallback, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { defaultProxyAddr, type FlowSource, type FlowSummary } from "./api";
import { toRow } from "./flow";
import { NotAnswering } from "./NotAnswering";
import { Button, DurationText, MethodLabel, Notice, StatusPill, TimestampText } from "./components/index";

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

// The region is empty for five unrelated reasons and none of them may share a
// sentence: telling someone to send traffic they have already sent, or to retry
// a query the core will refuse identically forever, is the failure each separate
// wording exists to prevent. A user who just cleared the capture is exactly the
// person the never-captured wording would insult, which is why clearing gets a
// heading of its own; the remedy is shared with it because after a clear it
// genuinely is the same remedy.
//
// A silent core outranks every empty, because an empty list means nothing when
// the list could not be fetched — that is the run's rule. It does NOT outrank a
// refusal: the run never ranked those two against each other, and they are not
// alternatives, so when both are true the refusal stands here and the shell
// states the silence above it rather than in its place. A clear ranks under both
// and over the two query absences: it is the only cause no property of the list
// can carry, but it is a fact about content, and a list that could not be
// fetched or a query that was refused has no content to explain.
function Absence({
  filtered,
  cleared,
  silent,
  refused,
  retrying,
  onRetry,
  onSetUp,
}: {
  filtered: boolean;
  cleared: boolean;
  silent: boolean;
  refused: string | null;
  retrying: boolean;
  onRetry: () => void;
  onSetUp?: () => void;
}) {
  const refill = `send traffic through the proxy — ${defaultProxyAddr()}`;
  return (
    <div data-testid="empty" className="flex justify-center p-12">
      {refused !== null ? (
        // A rejection, not a failure: the API answered and said no, so there is
        // no ⚠ and no retry — the correction is more typing.
        <Notice detail="type a code (404) or a class (4xx)">{`${refused} is not a status`}</Notice>
      ) : silent ? (
        // The clause is a claim about why this region is empty, so it is made
        // only when it is this region's own truth. Neither a filter that
        // excludes everything nor a capture the user just emptied is a window
        // nothing has reached — in both the traffic arrived and something else
        // is why the region is bare.
        <NotAnswering
          trailing={filtered || cleared ? undefined : "nothing has reached the window"}
          retrying={retrying}
          onRetry={onRetry}
        />
      ) : cleared ? (
        <Notice detail={refill}>capture cleared</Notice>
      ) : filtered ? (
        <Notice detail="clear a field to widen the search">no flows match this filter</Notice>
      ) : (
        // A window nothing has reached is the one empty whose remedy is setup,
        // so it offers the way there beside the dot (P4).
        <Notice detail={refill} action={onSetUp && <Button onClick={onSetUp}>set up capture</Button>}>
          no flows yet
        </Notice>
      )}
    </div>
  );
}

// The mark wears the door's own face — ↑ is the import control's, ↗ is new
// request's — and its box is reserved whether or not it is drawn, so every
// timestamp shares one left edge. Not a column and not a component: one
// conditional character with an accessible name, in one cell of one screen.
const SOURCE_MARK: Partial<Record<FlowSource, string>> = { imported: "↑", sent: "↗" };

function SourceMark({ source }: { source: FlowSource }) {
  const mark = SOURCE_MARK[source];
  return (
    <span aria-label={mark && source} title={mark && source} className="inline-block w-4 text-ink-mute">
      {mark ?? ""}
    </span>
  );
}

export function FlowList({
  flows,
  selectedId,
  onSelect,
  filtered = false,
  cleared = false,
  silent = false,
  refused = null,
  retrying = false,
  onRetry = () => {},
  onSetUp,
}: {
  flows: FlowSummary[];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  filtered?: boolean;
  cleared?: boolean;
  silent?: boolean;
  refused?: string | null;
  retrying?: boolean;
  onRetry?: () => void;
  onSetUp?: () => void;
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
        <Absence
          filtered={filtered}
          cleared={cleared}
          silent={silent}
          refused={refused}
          retrying={retrying}
          onRetry={onRetry}
          onSetUp={onSetUp}
        />
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
                  <SourceMark source={flow.source} />
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
