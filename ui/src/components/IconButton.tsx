import type { ReactNode } from "react";
import { Button } from "./Button";

export type GlyphName = "close" | "save" | "help" | "more" | "window" | "add" | "previous" | "next" | "replace";

const GLYPH: Record<GlyphName, ReactNode> = {
  close: "×",
  save: "↓",
  more: "⋯",
  window: "↗",
  add: "+",
  previous: "↑",
  next: "↓",
  replace: "⇄",
  help: (
    <svg viewBox="0 0 16 16" width="1em" height="1em" aria-hidden="true" data-glyph="help">
      <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" strokeWidth="1.25" />
      <text x="8" y="11.5" textAnchor="middle" fontSize="10" fill="currentColor">
        ?
      </text>
    </svg>
  ),
};

export interface IconButtonProps {
  glyph?: GlyphName;
  children?: ReactNode;
  label?: string;
  onClick: () => void;
  size?: number;
  disabled?: boolean;
  className?: string;
  expanded?: boolean;
  controls?: string;
}

/**
 * IconButton — bare-glyph button; compact affordance for icon-only actions (close, save, collapse).
 * @tier T1 — generic action affordance on a composed Button shell, no business logic
 * @anatomy a Button with frame="bare" shape="square" size={28} tone="mute" — 28×28 hit area (size overrides, floor 24), padded beyond the glyph · glyph from built-in registry (glyph="close"|"save"|"help"|"more"|"window"|"add"|"previous"|"next"|"replace": × ↓ ? ⋯ ↗ + ↑ ↓ ⇄, help a ? in a circle) or custom children · muted glyph at rest, ink + 7% wash on hover/active (the bare frame's hover signal) · focus ring, disabled treatment, radius and keyboard inherited from the shell · native title tooltip, defaults to the glyph name
 * @use icon-only triggers unambiguous in place: close ×, save ↓, help ? in a circle, a menu of more acts ⋯, open in data window ↗ (never ⧉, which is CopyButton's copy glyph), add a row +, step to the previous ↑ or next ↓ match beside a find box (next shares save's arrow: read in place, it is the find bar's), replace with the decoded form ⇄ — wherever the glyph alone carries the action
 * @avoid actions with no fitting glyph (word label carries it better — use Button, or CopyButton if it is a copy) · primary actions (no visual weight at rest)
 * @states rest (muted glyph, no bg) · hover (ink + 7% bg wash) · focus (1px mute outline) · active (ink + 13% bg) · disabled (35% opacity, not-allowed, no tab)
 * @madeof Button (T1) with frame="bare" shape="square" size={28} tone="mute" — the element, focus ring, disabled, radius, hit floor and keyboard come from the shell; this component adds the glyph registry and the accessible-name guarantee
 */
export function IconButton({
  glyph,
  children,
  label,
  onClick,
  size = 28,
  disabled = false,
  className = "",
  expanded,
  controls,
}: IconButtonProps) {
  const face = children ?? (glyph ? GLYPH[glyph] : null);

  return (
    <Button
      onClick={onClick}
      disabled={disabled}
      frame="bare"
      shape="square"
      tone="mute"
      size={size}
      label={label ?? glyph}
      expanded={expanded}
      controls={controls}
      className={`text-note ${className}`}
    >
      {face}
    </Button>
  );
}
