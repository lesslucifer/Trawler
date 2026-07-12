import type { InputHTMLAttributes, ReactNode } from "react";
import type { StatusTone } from "./flow";

// Base component kit — Warp-styled primitives later UI epics reuse. Kept to what
// the flow list needs today (a text field, a status pill, a live dot); grow it
// when a real second tenant needs more, not before.

export function Field(props: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      className={`rounded-button border border-hairline bg-canvas px-3 py-1 font-ui text-ink placeholder:text-ink-mute focus:border-ink-mute focus:outline-none ${props.className ?? ""}`}
    />
  );
}

const TONE_CLASS: Record<StatusTone, string> = {
  ok: "text-ok",
  warn: "text-warn",
  muted: "text-ink-mute",
};

export function StatusPill({ tone, children }: { tone: StatusTone; children: ReactNode }) {
  return <span className={`font-mono tabular-nums ${TONE_CLASS[tone]}`}>{children}</span>;
}

export function LiveDot({ live }: { live: boolean }) {
  return (
    <span
      data-testid="live"
      title={live ? "live stream connected" : "stream disconnected"}
      className={`inline-block h-2 w-2 rounded-full ${live ? "bg-ok" : "bg-warn"}`}
    />
  );
}
