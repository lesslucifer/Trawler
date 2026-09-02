import type { ReactNode } from "react";

export type NoticeTone = "mute" | "warn" | "ok";

const TONE: Record<NoticeTone, { text: string; glyph: string | null }> = {
  mute: { text: "text-ink-mute", glyph: null },
  warn: { text: "text-warn", glyph: "⚠" },
  ok: { text: "text-ok", glyph: "✓" },
};

/**
 * Notice — one sentence explaining an absence or a failure, with the action that helps if one exists.
 * @tier T1 — renders a sentence at a tone; owns no domain knowledge
 * @anatomy Inter 12px sentence at ink-mute, ink-warn or ok · ⚠ glyph at warn, ✓ at ok, none at mute (aria-hidden; colour is never the only channel) · optional detail line, always ink-mute, 11.5px · optional action slot 8px below, aligned to the sentence not the glyph · no border, no background, no panel chrome
 * @use any zone that has nothing to show and must say which nothing it is (empty · pending · capped · not captured) · any zone whose load failed · any zone whose whole content is a STANDING positive outcome (tone="ok" — "trawler is capturing") — HeaderTable empty/pending, BodyViewer's five absences, the detail panel's load failures, the setup screen's capture-is-working zone
 * @avoid transient confirmations of an action (use confirm-at-point at the trigger — CopyButton owns its own; tone="ok" is for a STANDING outcome, not an acknowledgement) · a dismissible message (dismissing leaves the blank zone this component exists to prevent) · a toned value rather than a sentence (that is TonedText, or StatusPill for an HTTP status) · a status verdict with a glyph+name+message shape (that is StatusLine) · wrapping a notice in a card or panel of its own
 * @states not interactive — no rest/hover/focus. Renders: mute bare · mute + detail · warn + ⚠ · warn + detail · ok + ✓ · ok + detail · any tone + action slot
 * @madeof Button (T1) — in the action slot only
 */
export function Notice({
  children,
  tone = "mute",
  detail,
  action,
  className = "",
}: {
  children: ReactNode;
  tone?: NoticeTone;
  detail?: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  const { text, glyph } = TONE[tone];

  return (
    <div data-tone={tone} className={`flex items-start gap-chrome-gap py-chrome-y ${className}`}>
      {glyph && (
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
