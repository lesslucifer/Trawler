import type { ChangeEvent, InputHTMLAttributes } from "react";

export interface FieldProps {
  value: string;
  onChange: (event: ChangeEvent<HTMLInputElement>) => void;
  "aria-label": string;
  placeholder?: string;
  className?: string;
  inputMode?: InputHTMLAttributes<HTMLInputElement>["inputMode"];
  disabled?: boolean;
}

/**
 * Field — the app's one text input: a labelled single-line box that reports what is typed and judges none of it.
 * @tier T1 — a controlled box with an accessible name; no validation, no debounce, no formatting, no state of its own
 * @anatomy the one small-control box (border-hairline, radius 3, padding 3/9, min-height 24 — Select's trigger and Button's shell, DEC-18) · placeholder in ink-mute as the visible hint · value in full ink · width is NEVER set here, the caller passes a field-sm/field/field-lg class
 * @use any free-text entry: the filter bar's free-text box and its status box, a search box, a name, a port override. Pass value + onChange + aria-label; pass width through className
 * @avoid a choice among known options (that is Select — this is a miss by kind: five options do not become typing) · validating, debouncing, formatting or restricting what is typed: an unmatched filter is an empty list, explained by Notice at the list (no invalid state exists, DEC-19 leaves no token for one) · rendering it without an accessible name — aria-label is required because every tenant is visually unlabelled and a placeholder is not a name · setting its own width, or letting it grow to fit content (the filter bar's four controls share one header row) · Select's in-popup filter box, which is borderless inside the popup's own frame and carries a name Select fixes rather than a caller supplies
 * @states rest·empty (placeholder, ink-mute) · rest·filled (value, ink) · hover and focus (both lift the border to ink-mute — one channel, zero-accent leaves no second) · disabled (opacity .38, out of tab order). No error, loading, empty or confirming state: empty IS rest, and nothing here is committed or judged
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
}: FieldProps) {
  return (
    <input
      type="text"
      value={value}
      onChange={onChange}
      aria-label={ariaLabel}
      placeholder={placeholder}
      inputMode={inputMode}
      disabled={disabled}
      className={`min-h-control rounded-button border border-hairline bg-canvas px-control-x py-control-y font-ui text-control text-ink outline-none transition-colors placeholder:text-ink-mute hover:border-ink-mute focus:border-ink-mute disabled:opacity-disabled ${className}`}
    />
  );
}
