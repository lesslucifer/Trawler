import { useRef, useState, type ReactNode } from "react";
import { Button, CopyText, Notice, StatusLine, TimestampText } from "./components/index";
import { stoppedWords, type CoreState } from "./coreService";
import { LogTail } from "./LogTail";
import { tauriBridge, useCoreLog, useCoreService, type CoreBridge } from "./useCoreService";
import type { StepStatusView } from "./verdicts";

type Running = Extract<CoreState, { state: "running" }>;

function verdict(state: CoreState | null): StepStatusView {
  switch (state?.state) {
    case "running":
      return { glyph: "✓", tone: "ok", short: "running" };
    case "starting":
      return { glyph: "⟳", tone: "muted", short: "starting" };
    case "exited":
      return { glyph: "⚠", tone: "warn", short: `stopped — ${stoppedWords(state)}` };
    case "attached":
      return { glyph: "◐", tone: "muted", short: "attached — started outside this app" };
    default:
      return { glyph: "○", tone: "muted", short: "not read yet" };
  }
}

function Fact({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div className="flex items-baseline gap-3">
      <dt className="w-field shrink-0 text-ink-mute">{name}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  );
}

function Address({ what, value }: { what: string; value: string }) {
  return (
    <span data-wire={what} className="font-mono">
      <CopyText value={value} label={what} />
    </span>
  );
}

function Facts({ state, lastRunning }: { state: CoreState | null; lastRunning: Running | null }) {
  if (state?.state === "attached") {
    return (
      <dl className="flex flex-col gap-grid-gap-y">
        <Fact name="control API">
          <Address what="control API address" value={state.api} />
        </Fact>
      </dl>
    );
  }
  const addresses = state?.state === "running" ? state : lastRunning;
  if (addresses === null) return null;
  return (
    <dl className="flex flex-col gap-grid-gap-y">
      {state?.state === "running" && (
        <>
          <Fact name="process">
            <span data-wire="process id" className="font-mono">
              {state.pid}
            </span>
          </Fact>
          <Fact name="running since">
            <TimestampText timestamp={state.started_at_ms} />
          </Fact>
        </>
      )}
      <Fact name="proxy">
        <Address what="proxy address" value={addresses.proxy} />
      </Fact>
      <Fact name="control API">
        <Address what="control API address" value={addresses.api} />
      </Fact>
    </dl>
  );
}

export function CaptureService({ bridge = tauriBridge }: { bridge?: CoreBridge }) {
  const { state, restarting, refusal, restart } = useCoreService(bridge);
  const entries = useCoreLog(bridge);
  const [asking, setAsking] = useState(false);
  const lastRunning = useRef<Running | null>(null);
  const restartHolder = useRef<HTMLDivElement>(null);
  if (state?.state === "running") lastRunning.current = state;

  const confirm = () => {
    setAsking(false);
    void restart();
  };
  const cancel = () => {
    setAsking(false);
    requestAnimationFrame(() => restartHolder.current?.querySelector("button")?.focus());
  };
  const pressRestart = () => (state?.state === "running" ? setAsking(true) : void restart());

  const waiting = restarting || state?.state === "starting";
  let actions: ReactNode;
  if (state?.state === "attached") {
    actions = (
      <Notice tone="mute">
        This app didn't start the capture service, so it can't restart it or show its log.
      </Notice>
    );
  } else if (waiting) {
    actions = (
      <div className="flex items-center gap-3">
        <Button onClick={pressRestart} disabled>
          restart
        </Button>
        <Notice tone="pending">waiting for the capture service to start…</Notice>
      </div>
    );
  } else if (refusal !== null) {
    actions = (
      <Notice tone="warn" detail={refusal} action={<Button onClick={confirm}>try again</Button>}>
        The capture service couldn't restart
      </Notice>
    );
  } else if (asking) {
    actions = (
      <div
        onKeyDown={(event) => {
          if (event.key === "Escape") cancel();
        }}
      >
        <Notice
          tone="warn"
          detail="Every flow captured so far lives in the capture service and goes with it."
          action={
            <div className="flex gap-chrome-gap">
              <Button onClick={confirm}>restart</Button>
              <Button onClick={cancel} frame="bare" tone="mute">
                cancel
              </Button>
            </div>
          }
        >
          Restarting clears the captured list
        </Notice>
      </div>
    );
  } else if (state !== null) {
    actions = (
      <div ref={restartHolder}>
        <Button onClick={pressRestart}>restart</Button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 bg-canvas p-4 font-ui text-body text-ink">
      <h1 className="font-serif text-title italic">Capture service</h1>
      <StatusLine status={verdict(state)} />
      <Facts state={state} lastRunning={lastRunning.current} />
      {actions && <div className="mb-2">{actions}</div>}
      {state?.state !== "attached" && <LogTail entries={entries} />}
    </div>
  );
}
