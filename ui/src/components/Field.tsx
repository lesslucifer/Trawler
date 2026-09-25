import type { ChangeEvent, InputHTMLAttributes } from "react";

export interface FieldProps {
  value: string;
  onChange: (event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => void;
  "aria-label": string;
  placeholder?: string;
  className?: string;
  inputMode?: InputHTMLAttributes<HTMLInputElement>["inputMode"];
  disabled?: boolean;
  readOnly?: boolean;
  multiline?: boolean;
  verbatim?: boolean;
}

const BOX =
  "min-h-control rounded-button border border-hairline bg-canvas px-control-x py-control-y text-control text-ink outline-none transition-colors placeholder:text-ink-mute hover:border-ink-mute focus:border-ink-mute disabled:opacity-disabled read-only:opacity-disabled";

/**
 * Field — the app's one text input: a labelled box, single-line or multi-line, that reports what is typed and judges none of it.
 * @tier T1 — a controlled box with an accessible name; no validation, no debounce, no formatting, no state of its own
 * @anatomy the one small-control box (border-hairline, radius 3, padding 3/9, min-height 24 — Select's trigger and Button's shell, DEC-18) · placeholder in ink-mute as the visible hint · value in full ink, in the ui family, or the mono family when `verbatim` (a URL, a header, a body: a value the wire carries as typed) · width is NEVER set here, the caller passes a field-sm/field/field-lg class · `multiline` is a textarea in the same box: its height is the caller's class too, it never resizes by hand, and its text scrolls inside
 * @use any free-text entry: the filter bar's free-text box and its status box, a search box, a name, a port override; `multiline` for text that runs to lines (a pasted curl command, a request body); `verbatim` for a value sent as typed. Pass value + onChange + aria-label; pass width and a multi-line height through className
 * @avoid a choice among known options (that is Select — this is a miss by kind: five options do not become typing) · validating, debouncing, formatting or restricting what is typed: an unmatched filter is an empty list, explained by Notice at the list (no invalid state exists, DEC-19 leaves no token for one) · marking a value edited against where it started: that is a comparison, the screen's to draw beside the box in words (DEC-59) · rendering it without an accessible name — aria-label is required because every tenant is visually unlabelled and a placeholder is not a name · setting its own width or height, or letting it grow to fit content (the filter bar's four controls share one header row) · a second hand-made textarea beside it (DEC-59) · Select's in-popup filter box, which is borderless inside the popup's own frame and carries a name Select fixes rather than a caller supplies
 * @states rest·empty (placeholder, ink-mute) · rest·filled (value, ink) · hover and focus (both lift the border to ink-mute — one channel, zero-accent leaves no second) · disabled (opacity .38, out of tab order) · read-only (opacity .38, still focusable — the hold a caller places on the box while the act it feeds runs). No error, loading, empty, edited or confirming state: empty IS rest, and nothing here is committed or judged
 * @madeof — (the leaf; its box is Select's trigger box, which is Button's — one shared small-control chrome per DEC-18, not a composition)
 */
export function Field({
  value,
  onChange,
  "aria-label": ariaLabel,
  placeholder,
  className = "",
  inputMode,
  disabled = false,
  readOnly = false,
  multiline = false,
  verbatim = false,
}: FieldProps) {
  const family = verbatim ? "font-mono" : "font-ui";
  const shared = { value, onChange, placeholder, disabled, readOnly, "aria-label": ariaLabel };
  return multiline ? (
    <textarea {...shared} className={`${BOX} ${family} block resize-none ${className}`} />
  ) : (
    <input type="text" {...shared} inputMode={inputMode} className={`${BOX} ${family} ${className}`} />
  );
}
