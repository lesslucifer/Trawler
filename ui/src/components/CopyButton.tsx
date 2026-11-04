import type { ReactNode } from "react";
import { Button } from "./Button";
import { useConfirm } from "./useConfirm";

export interface CopyButtonProps {
  label: string;
  value?: string;
  getPayload?: () => string;
  icon?: ReactNode;
  iconOnly?: boolean;
  disabled?: boolean;
  className?: string;
}

const COPY_GLYPH = "⧉";

export function writeClipboard(text: string): Promise<void> {
  return Promise.resolve(navigator.clipboard?.writeText(text));
}

/**
 * CopyButton — labeled button that copies a derived payload and confirms inline; payload is not displayed.
 * @tier T1 — clipboard write + timed label-swap on a composed Button shell, no business logic
 * @anatomy two forms — LABELLED (default): compact bordered button · label (names payload) → "copied" (ok) or "failed" (warn) for ~2 s → reverts · optional leading icon. ICON-ONLY (fork D2b, adjacency test): 24px square · ⧉ glyph · tooltip + aria-label mandatory · ✓ ok-tone confirm, same ~2 s timer rules
 * @use any one-click copy affordance where the payload is derived/invisible: the detail panel's copy-as-curl, BodyViewer's copy body
 * @avoid showing a value the user grabs verbatim (that is CopyText) · icon-only form UNLESS the payload is rendered directly adjacent (fork D2b: BodyViewer's pane — 24px square, mandatory tooltip + aria-label, ✓ confirm) · anything that is not a copy action · serialising the payload itself (the parent owns press logic)
 * @states rest · hover · focus · active · confirming (~2 s, restarts on re-click, resets instantly on payload change — label swaps to "copied", or the glyph swaps to ✓ in the icon-only form) · error ("failed", warn tone, ~2 s, same timer rules; ⚠ glyph in the icon-only form) · disabled (nothing to copy)
 * @madeof Button (T1) with tone="mute", plus shape="square" for the D2b icon form — the element, focus ring, disabled, radius, hit floor and keyboard come from the shell · the shared ~2 s confirm hook (with CopyText). The copy contract itself lives here — fork D4
 */
export function CopyButton({
  label,
  value,
  getPayload,
  icon,
  iconOnly = false,
  disabled = false,
  className = "",
}: CopyButtonProps) {
  const { state, flash } = useConfirm(value);

  const copy = () => {
    const payload = getPayload ? getPayload() : (value ?? "");
    writeClipboard(payload).then(
      () => flash("confirming"),
      () => flash("error"),
    );
  };

  const outcomeClass =
    state === "confirming"
      ? "text-ok border-ok hover:text-ok"
      : state === "error"
        ? "text-warn border-warn hover:text-warn"
        : "";

  const glyph = iconOnly
    ? state === "confirming"
      ? "✓"
      : state === "error"
        ? "⚠"
        : (icon ?? COPY_GLYPH)
    : icon;

  const text = state === "confirming" ? "copied" : state === "error" ? "failed" : label;
  const widest = [label, "copied", "failed"].reduce((a, b) => (b.length > a.length ? b : a));

  return (
    <>
      <Button
        onClick={copy}
        disabled={disabled}
        tone="mute"
        shape={iconOnly ? "square" : "label"}
        glyph={glyph}
        label={iconOnly ? label : undefined}
        className={`${outcomeClass} ${className}`}
      >
        {iconOnly ? undefined : (
          <span className="grid">
            <span aria-hidden="true" className="invisible col-start-1 row-start-1">
              {widest}
            </span>
            <span className="col-start-1 row-start-1">{text}</span>
          </span>
        )}
      </Button>
      <span className="sr-only" aria-live="polite">
        {state === "confirming" ? `copied ${label}` : state === "error" ? "copy failed" : ""}
      </span>
    </>
  );
}
