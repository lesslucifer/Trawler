import type { ReactNode } from "react";
import { Spinner } from "./Spinner";

export type NoticeTone = "mute" | "warn" | "ok" | "pending";
export type NoticeLayout = "inline" | "region";

const TONE: Record<NoticeTone, { text: string; glyph: ReactNode }> = {
  mute: { text: "text-ink-mute", glyph: null },
  warn: { text: "text-warn", glyph: "⚠" },
  ok: { text: "text-ok", glyph: "✓" },
  pending: { text: "text-ink-mute", glyph: <Spinner /> },
};

/**
 * Notice — one sentence explaining an absence or a failure, with the action that helps if one exists.
 * @tier T1 — renders a sentence at a tone; owns no domain knowledge
 * @anatomy inline (the default): Inter 12px sentence at ink-mute, ink-warn or ok · ⚠ glyph at warn, ✓ at ok, a spinner at pending, none at mute (aria-hidden; colour is never the only channel) · optional detail line, always ink-mute, 11.5px · optional action slot 8px below, aligned to the sentence not the glyph · no border, no background, no panel chrome. region: the same parts stacked and centred — a 14px sentence (ink at mute, so a whole region's one statement reads first), the glyph on its line, the detail at 12px capped to the reading width, the action slot centred 8px below — for a notice that is the whole of its region
 * @use layout="region" where the notice is all a region holds — the flow list's empties, the core not answering in place of the rows or of setup's tasks; inline everywhere else · any zone that has nothing to show and must say which nothing it is (empty · pending · capped · not captured) · any zone whose load failed · any zone whose whole content is a STANDING positive outcome (tone="ok") — the detail panel's empty-headers zone and its loading line, BodyViewer's five absences and its load failure, the flow list's two empties, the setup screen's "installed" and "proxy set", the quit dialog's "no longer routes through Trawler"
 * @avoid layout="region" for a notice beside other content (a band over rows, a line inside a task — it would pull the eye off what it qualifies) · transient confirmations of an action (use confirm-at-point at the trigger — CopyButton owns its own; tone="ok" is for a STANDING outcome, not an acknowledgement) · a dismissible message (dismissing leaves the blank zone this component exists to prevent) · a toned value rather than a sentence (that is TonedText, or StatusPill for an HTTP status) · a status verdict with a glyph+name+message shape (that is StatusLine) · wrapping a notice in a card or panel of its own
 * @states not interactive — no rest/hover/focus. Renders, in either layout: mute bare · mute + detail · warn + ⚠ · warn + detail · ok + ✓ · ok + detail · pending + spinner (a zone still resolving; the sentence and detail stay at ink-mute, so it reads as working-on-it rather than as settled) · any tone + action slot
 * @madeof Button (T1) — in the action slot only · Spinner (T1) — the pending glyph
 */
export function Notice({
  children,
  tone = "mute",
  detail,
  action,
  layout = "inline",
  className = "",
}: {
  children: ReactNode;
  tone?: NoticeTone;
  detail?: ReactNode;
  action?: ReactNode;
  layout?: NoticeLayout;
  className?: string;
}) {
  const { text, glyph } = TONE[tone];

  if (layout === "region") {
    return (
      <div
        data-tone={tone}
        data-layout="region"
        className={`flex flex-col items-center gap-chrome-gap py-chrome-y text-center ${className}`}
      >
        <p className={`font-ui text-note leading-normal ${tone === "mute" ? "text-ink" : text}`}>
          {glyph !== null && (
            <span aria-hidden="true" className="mr-chrome-gap">
              {glyph}
            </span>
          )}
          {children}
        </p>
        {detail !== undefined && (
          <p className="max-w-reading font-ui text-body leading-normal text-ink-mute">{detail}</p>
        )}
        {action !== undefined && <div className="mt-2 flex justify-center gap-2">{action}</div>}
      </div>
    );
  }

  return (
    <div data-tone={tone} className={`flex items-start gap-chrome-gap py-chrome-y ${className}`}>
      {glyph !== null && (
        <span aria-hidden="true" className={`shrink-0 font-ui text-body ${text}`}>
          {glyph}
        </span>
      )}
      <div className="min-w-0">
        <p className={`font-ui text-body leading-normal ${text}`}>{children}</p>
        {detail !== undefined && (
          <p className="mt-chrome-tight font-ui text-control text-ink-mute">{detail}</p>
        )}
        {action !== undefined && <div className="mt-2 flex gap-2">{action}</div>}
      </div>
    </div>
  );
}
