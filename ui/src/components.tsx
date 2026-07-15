import type { InputHTMLAttributes, ReactNode } from "react";
import type { StatusTone } from "./flow";
import { COPY, type DotStatus } from "./onboardingMachine";

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

// Capture-status dot in the app header — the single connection indicator AND
// the always-available setup entry (tap to re-invoke onboarding). Supersedes the
// old 2-state stream LiveDot. checking has no chromatic token (Warp DEC-11), so
// it reads muted; broken/connected reuse warn/ok.
const DOT_CLASS: Record<DotStatus, string> = {
  broken: "bg-warn",
  checking: "bg-ink-mute",
  connected: "bg-ok",
};

export function StatusDot({ status, onClick }: { status: DotStatus; onClick: () => void }) {
  return (
    <button
      type="button"
      data-testid="status-dot"
      data-status={status}
      title={COPY.dotTitle[status]}
      onClick={onClick}
      className={`inline-block h-2.5 w-2.5 shrink-0 rounded-full ${DOT_CLASS[status]}`}
    />
  );
}
