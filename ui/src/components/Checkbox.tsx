import type { ReactNode } from "react";

export interface CheckboxProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  children: ReactNode;
  disabled?: boolean;
  className?: string;
}

/**
 * Checkbox — the app's one boolean control: a durable preference the user sets, with its label inside the hit target.
 * @tier T1 — a controlled box plus its label; no tri-state, no validation, no state of its own
 * @anatomy a label element wrapping the native input and the text, so the words are part of the target (chrome-gap between them) · the native box at its platform size, accent-neutral · label in Inter ink-mute at body size · the whole row is the hit area, never the 13px box alone
 * @use a durable boolean preference the user sets and the app remembers — the quit dialog's "don't show this again". Pass checked + onChange + the label as children
 * @avoid a mutually exclusive view or mode switch (that is SegmentedControl — its @use is an always-visible exclusive switch, which a single boolean is not) · free text (that is Field) · a per-step completion tick (StatusLine @avoid: a tick lies in most real states) · a tri-state or indeterminate box: there is no third value here · rendering the label outside the target, which halves the hit area and breaks generous-targets · using it to fire an action — a checkbox records a preference, a Button acts
 * @states unchecked · checked · hover and focus (the border lifts to ink-mute, one channel) · disabled (opacity .38, out of tab order). No error, loading or indeterminate state
 * @madeof —
 */
export function Checkbox({
  checked,
  onChange,
  children,
  disabled = false,
  className = "",
}: CheckboxProps) {
  return (
    <label
      className={`inline-flex min-h-control cursor-pointer items-center gap-chrome-gap font-ui text-body text-ink-mute has-[:disabled]:cursor-default has-[:disabled]:opacity-disabled ${className}`}
    >
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => !disabled && onChange(event.target.checked)}
        className="accent-ink-mute outline-none"
      />
      {children}
    </label>
  );
}
