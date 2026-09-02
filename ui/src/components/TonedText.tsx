import type { ReactNode } from "react";
import type { StatusTone } from "../flow";

const TONE: Record<StatusTone, string> = {
  ok: "text-ok",
  warn: "text-warn",
  muted: "text-ink-mute",
};

/**
 * TonedText — a short value or message in one of the app's three semantic tones; the tone channel with the meaning removed.
 * @tier T1 — tone in, text in, span out: no mapping, no formatting, no state. The base StatusPill and StatusLine are made of
 * @anatomy inline mono span with tabular-nums at inherited size (components.tsx:25) · tone → text-ok / text-warn / text-ink-mute, a three-key lookup and nothing else · children rendered as given · no background, border, radius, padding, role or title
 * @use any short value whose meaning is carried by tone and that is not an HTTP status — a verdict glyph + phrase, a toned count, a health word. Pass the resolved tone; the tone union is StatusTone / StepTone, already shipped
 * @avoid an HTTP status (use StatusPill — it owns the status→tone mapping) · a toned sentence or prose with an action (use Notice — Inter, message, action slot) · relying on colour alone: the caller must carry a second channel (glyph or word), which this component states but cannot enforce · resolving tone inside a call site that already has a T2 for it · making it interactive — wrap it in CopyText or Button instead
 * @states tone:ok · tone:warn · tone:muted — the tone prop is the whole state machine. No interaction, empty, in-flight or transition states; a pending value is a muted tone plus the caller's own placeholder text
 * @madeof —
 */
export function TonedText({
  tone,
  children,
  className = "",
}: {
  tone: StatusTone;
  children: ReactNode;
  className?: string;
}) {
  return <span className={`font-mono tabular-nums ${TONE[tone]} ${className}`}>{children}</span>;
}
