import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { FlowSummary } from "./api";
import { toRow } from "./flow";
import { StatusPill } from "./components";

const ROW_HEIGHT = 30;

const COLUMNS = "110px 70px 180px 1fr 70px 90px";

function HeaderRow() {
  return (
    <div
      className="grid gap-3 border-b border-hairline px-4 py-1 text-xs uppercase tracking-wide text-ink-mute"
      style={{ gridTemplateColumns: COLUMNS }}
    >
      <span>Time</span>
      <span>Method</span>
      <span>Host</span>
      <span>Path</span>
      <span className="text-right">Status</span>
      <span className="text-right">Duration</span>
    </div>
  );
}

export function FlowList({ flows }: { flows: FlowSummary[] }) {
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: flows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  if (flows.length === 0) {
    return (
      <div className="flex h-full flex-col">
        <HeaderRow />
        <p data-testid="empty" className="p-12 text-center text-ink-mute">
          no flows yet — send traffic through the proxy
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <HeaderRow />
      <div ref={parentRef} data-testid="flow-scroll" className="min-h-0 flex-1 overflow-auto">
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const row = toRow(flows[item.index]);
            return (
              <div
                key={flows[item.index].id}
                data-testid="flow-row"
                className="absolute left-0 top-0 grid w-full items-center gap-3 border-b border-hairline px-4 font-mono text-ink-body"
                style={{
                  gridTemplateColumns: COLUMNS,
                  height: ROW_HEIGHT,
                  transform: `translateY(${item.start}px)`,
                }}
              >
                <span className="truncate text-ink-mute">{row.time}</span>
                <span className="truncate">{row.method}</span>
                <span className="truncate">{row.host}</span>
                <span className="truncate" title={row.path}>{row.path}</span>
                <span className="text-right">
                  <StatusPill tone={row.tone}>{row.status}</StatusPill>
                </span>
                <span className="text-right tabular-nums text-ink-mute">{row.duration}</span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
