import { useCallback, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { FlowSource, FlowSummary } from "./api";
import { toRow } from "./flow";
import { NotAnswering } from "./NotAnswering";
import { Button, DurationText, MethodLabel, Notice, Splitter, StatusPill, TimestampText } from "./components/index";
import { COLUMN_BOUNDS, DEFAULT_COLUMNS, columnTemplate, listMin, type Track } from "./layoutBounds";
import type { LayoutControls } from "./useLayout";

const ROW_HEIGHT = 30;

type ColumnLabel = { track: Track; label: string; align?: string };

const BEFORE_PATH: readonly ColumnLabel[] = [
  { track: "time", label: "Time" },
  { track: "method", label: "Method" },
  { track: "host", label: "Host" },
];
const AFTER_PATH: readonly ColumnLabel[] = [
  { track: "status", label: "Status", align: "text-center" },
  { track: "duration", label: "Duration", align: "text-right" },
];

function ColumnHandle({ column, edge, split }: { column: ColumnLabel; edge: "left" | "right"; split: LayoutControls }) {
  const width = split.layout.columns[column.track];
  const { min, max } = COLUMN_BOUNDS[column.track];
  return (
    <div className={`absolute inset-y-0 flex w-3 justify-center ${edge === "right" ? "-right-3" : "-left-3"}`}>
      <Splitter
        axis="x"
        dir={edge === "right" ? 1 : -1}
        value={width}
        min={min}
        max={max}
        onChange={(next) => split.setColumn(column.track, next)}
        onCommit={split.commit}
        onReset={() => split.resetColumn(column.track)}
        label={`${column.label.toLowerCase()} column width`}
        valueText={`${width} px`}
      />
    </div>
  );
}

function HeaderRow({ template, split }: { template: string; split?: LayoutControls }) {
  const label = (column: ColumnLabel, edge: "left" | "right") => (
    <span key={column.track} className={`relative ${column.align ?? ""}`}>
      {column.label}
      {split && <ColumnHandle column={column} edge={edge} split={split} />}
    </span>
  );
  return (
    <div
      data-wire="none"
      data-testid="column-labels"
      className="grid gap-3 border-b border-hairline px-4 py-chrome-y text-body uppercase tracking-label text-ink-mute"
      style={{ gridTemplateColumns: template }}
    >
      {BEFORE_PATH.map((column) => label(column, "right"))}
      <span>Path</span>
      {AFTER_PATH.map((column) => label(column, "left"))}
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
  return (
    <div data-testid="empty" className="flex flex-1 items-center justify-center p-12">
      {refused !== null ? (
        // A rejection, not a failure: the API answered and said no, so there is
        // no ⚠ and no retry — the correction is more typing.
        <Notice layout="region" detail="Try a code like 404, or a range like 4xx.">
          {`“${refused}” isn't a status code`}
        </Notice>
      ) : silent ? (
        // The clause is a claim about why this region is empty, so it is made
        // only when it is this region's own truth. Neither a filter that
        // excludes everything nor a capture the user just emptied is a window
        // nothing has reached — in both the traffic arrived and something else
        // is why the region is bare.
        <NotAnswering
          trailing={filtered || cleared ? undefined : "Nothing has been captured yet. Requests show up here once it's back."}
          region
          retrying={retrying}
          onRetry={onRetry}
        />
      ) : cleared ? (
        <Notice layout="region" detail="New requests will show up here as they arrive.">
          All cleared
        </Notice>
      ) : filtered ? (
        <Notice layout="region" detail="Try a shorter search, or clear one of the fields above.">
          Nothing matches your filter
        </Notice>
      ) : (
        // A window nothing has reached is the one empty whose remedy is setup,
        // so it offers the way there beside the dot (P4).
        <Notice
          layout="region"
          detail="Requests from your apps and devices show up here once they go through Trawler. Setup walks you through it, step by step."
          action={onSetUp && <Button onClick={onSetUp}>set up capture</Button>}
        >
          Nothing captured yet
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
  split,
}: {
  split?: LayoutControls;
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
  const columns = split?.layout.columns ?? DEFAULT_COLUMNS;
  const template = columnTemplate(columns);
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
        <HeaderRow template={template} split={split} />
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
    <div data-testid="flow-columns" className="h-full overflow-x-auto overflow-y-hidden">
      <div className="flex h-full flex-col" style={{ minWidth: listMin(columns) }}>
        <HeaderRow template={template} split={split} />
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
          className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden"
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
                    gridTemplateColumns: template,
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
    </div>
  );
}
