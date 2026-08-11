import type { ReactNode } from "react";

export type ButtonFrame = "border" | "bare";
export type ButtonShape = "label" | "square";
export type ButtonTone = "body" | "mute";

export interface ButtonProps {
  children?: ReactNode;
  onClick: () => void;
  glyph?: ReactNode;
  frame?: ButtonFrame;
  shape?: ButtonShape;
  tone?: ButtonTone;
  size?: number;
  disabled?: boolean;
  type?: "button" | "submit";
  className?: string;
  label?: string;
  title?: string;
}

const BASE =
  "inline-flex items-center justify-center rounded-button font-ui text-control cursor-pointer bg-transparent select-none whitespace-nowrap transition-colors hover:text-ink focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute focus-visible:outline-offset-focus focus-visible:text-ink disabled:opacity-disabled disabled:pointer-events-none";

const FRAME: Record<ButtonFrame, string> = {
  border: "border border-hairline hover:border-ink-mute active:bg-wash-tint active:text-ink",
  bare: "border-none hover:bg-wash-hover active:bg-wash-active active:text-ink",
};

const TONE: Record<ButtonTone, string> = {
  body: "text-ink-body",
  mute: "text-ink-mute",
};

const SQUARE_FLOOR = 24;

/**
 * Button — the collection's only <button> element: a labelled action by default, and the shell its two siblings compose.
 * @tier T1 — generic action affordance, no business logic
 * @anatomy 1px line border, radius 3, padding 3/9, min-height 24 for WCAG 2.5.8 · Inter 11.5px lowercase label · optional leading glyph (aria-hidden, rarely earned) · no filled or primary variant — zero-accent leaves nothing to fill it with. THREE CUSTOMIZATION AXES, defaults are Button's own: frame=border|bare (bare drops the border and hovers as a 7% wash) · shape=label|square (square takes size px per side, mandatory aria-label) · tone=body|mute (resting content colour). Everything else — focus ring, disabled, radius, hit floor, keyboard — is inherited and not overridable
 * @use any action that needs a word and is not a copy: retry, close, save, submit. Also the shell every other button composes — CopyButton (tone=mute, plus shape=square for its icon form) and IconButton (frame=bare, shape=square, size=28, tone=mute) — so one <button> element carries the focus ring, hit floor and keyboard behaviour for all three
 * @avoid copy actions (use CopyButton — it owns the clipboard write and its confirm) · actions whose glyph is unambiguous alone (use IconButton) · emphasis by styling: pick which actions to offer instead, a Button never carries a primary/filled variant · warn-toning it inside a warn Notice (the message owns the alarm, the button is the way out) · leaving it enabled while its action is in flight, or disabling it without a progress line beside it (fork D4 — both halves or neither)
 * @states rest (ink-body label, hairline border) · hover (ink label, mute border) · focus (1px mute outline, 2px offset) · active (12% mute wash) · disabled (38% opacity, no tab). No confirming and no loading state of its own — for a slow action the caller MUST render both halves (fork D4): this Button disabled for the whole in-flight window, AND a progress line in the same zone naming what is awaited
 * @madeof — (the leaf: nothing composes below it. Its two siblings compose IT — see @use)
 */
export function Button({
  children,
  onClick,
  glyph,
  frame = "border",
  shape = "label",
  tone = "body",
  size = SQUARE_FLOOR,
  disabled = false,
  type = "button",
  className = "",
  label,
  title,
}: ButtonProps) {
  const square = shape === "square";
  const side = Math.max(size, SQUARE_FLOOR);
  const box = square
    ? "p-0 gap-0 shrink-0"
    : "gap-control-gap px-control-x py-control-y min-h-control";

  return (
    <button
      type={type}
      disabled={disabled}
      onClick={onClick}
      aria-label={label}
      title={title ?? label}
      style={square ? { width: side, height: side } : undefined}
      className={`${BASE} ${box} ${FRAME[frame]} ${TONE[tone]} ${className}`}
    >
      {glyph !== undefined && (
        <span aria-hidden="true" className={`leading-none ${square ? "" : "text-body"}`}>
          {glyph}
        </span>
      )}
      {children}
    </button>
  );
}
