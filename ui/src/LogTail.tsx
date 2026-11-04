import { useLayoutEffect, useRef, useState } from "react";
import { Button, TimestampText } from "./components/index";
import type { LogEntry } from "./coreService";

const BOTTOM_SLACK_PX = 2;

function atBottom(element: HTMLElement): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight <= BOTTOM_SLACK_PX;
}

export function LogTail({ entries }: { entries: LogEntry[] }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [following, setFollowing] = useState(true);

  useLayoutEffect(() => {
    const element = scroller.current;
    if (element && following) element.scrollTop = element.scrollHeight;
  }, [entries, following]);

  const showNewest = () => {
    const element = scroller.current;
    if (element) element.scrollTop = element.scrollHeight;
    setFollowing(true);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-chrome-gap">
      <div className="text-chrome uppercase tracking-label text-ink-mute">log</div>
      <div
        ref={scroller}
        role="log"
        aria-label="capture service log"
        tabIndex={0}
        onScroll={(event) => setFollowing(atBottom(event.currentTarget))}
        className="min-h-0 flex-1 overflow-auto rounded-card border border-hairline bg-canvas-soft p-2 focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute"
        data-wire="collection"
        data-order="wire"
      >
        {entries.map((entry) =>
          entry.kind === "start" ? (
            <div
              key={entry.seq}
              data-testid="log-start"
              className="my-chrome-y border-t border-hairline pt-chrome-y font-ui text-chrome text-ink-mute"
            >
              started <TimestampText timestamp={entry.at_ms} /> · process {entry.pid}
            </div>
          ) : (
            <div
              key={entry.seq}
              data-wire="log line"
              className="whitespace-pre-wrap break-words font-mono text-chrome text-ink-body"
            >
              {entry.text}
            </div>
          ),
        )}
      </div>
      {!following && (
        <div>
          <Button onClick={showNewest}>show newest</Button>
        </div>
      )}
    </div>
  );
}
