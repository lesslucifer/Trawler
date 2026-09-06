import type { StepStatusView, StepTone } from "../setup";
import { TonedText } from "./TonedText";

const IN_FLIGHT_GLYPH = "⟳";

// The message carries the verdict's tone alongside the glyph, so ok and warn are
// distinguishable without reading a 9px mark. Muted stays ink-mute: only the two
// decided verdicts earn colour.
const MESSAGE_TONE: Record<StepTone, string> = {
  ok: "text-ok",
  warn: "text-warn",
  muted: "text-ink-mute",
};

/**
 * StatusLine — a precondition's resolved verdict on one line: glyph, name, short message.
 * @tier T2 — owns the verdict's form and the guarantee of its precedence (running > authoritative positive > observed proof > action error > partial evidence > can't-check > not-yet), plus the glyph→tone pairing. Takes a resolved StepStatusView; the domain resolvers own which verdict is true
 * @anatomy one baseline row: glyph as a TonedText in the verdict tone (✓ resolved · ⚠ attention · ◐ can't-determine · ○ not-yet · ⟳ in-flight, animated) · optional name in Inter ink-body, never toned, never truncated · the verdict in Inter carrying the verdict tone (ok and warn coloured, muted stays ink-mute — the glyph is the second channel), read from StepStatusView.short, a fragment not a sentence (≤ ~40 chars), wraps rather than truncates
 * @use any item that reports a resolved state with a reason — a setup precondition, a health check, a diagnostic row. Pass the resolver's StepStatusView through unchanged
 * @avoid an HTTP status (use StatusPill) · a zone-level absence or one-sentence outcome with an action (use Notice) · a bare toned value with no verdict semantics (use TonedText) · re-resolving or re-ranking the verdict at the call site — the order is the contract · shortening a domain message at the call site: the short form is the domain's to return, as StepStatusView.short · rendering the remedy: a verdict plus an instruction is two elements — pass StepStatusView.remedy to a Notice · rendering it as a checkbox or toggle: a tick lies in most real states, and it is pure display
 * @states ok (✓, resolved yes) · warn (⚠, needs attention) · muted·can't-determine (◐) · muted·not-yet (○) · in-flight (spinner, muted, outranks every other verdict including a stale error) — each with or without the name. No empty, disabled, interaction, or distinct-error state
 * @madeof TonedText
 */
export function StatusLine({
  status,
  name,
  className = "",
}: {
  status: StepStatusView;
  name?: string;
  className?: string;
}) {
  const inFlight = status.glyph === IN_FLIGHT_GLYPH;

  return (
    <div
      className={`flex min-h-line items-baseline gap-chrome-gap text-body leading-normal ${className}`}
    >
      {inFlight ? (
        <span
          aria-hidden="true"
          data-testid="status-line-spinner"
          className="inline-block h-spinner w-spinner animate-spin rounded-full border-emphasis border-hairline border-t-ink-mute"
        />
      ) : (
        <span aria-hidden="true">
          <TonedText tone={status.tone} className="text-body leading-none">
            {status.glyph}
          </TonedText>
        </span>
      )}
      {name && <span className="shrink-0 font-ui text-ink-body">{name}</span>}
      <span className={`min-w-0 font-ui ${MESSAGE_TONE[status.tone]}`}>{status.short}</span>
    </div>
  );
}
