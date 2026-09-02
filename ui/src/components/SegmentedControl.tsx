import { useRef } from "react";

export interface SegmentOption {
  value: string;
  label: string;
}

/**
 * SegmentedControl — single-select segment group: N mutually exclusive options,
 *   one always active, the state legible at a glance.
 * @tier T1 — selection display + click/keyboard handling only; what the options
 *   mean is the tenant's business logic
 * @anatomy bordered inline group · hairline dividers · active segment filled soft,
 *   inactive muted · radiogroup semantics, roving tabindex
 * @use any always-visible mutually exclusive view/mode switch: decode (raw | decoded)
 *   · leg (request | response)
 * @avoid actions (that is a button) · multi-select · tabs that own panels (tablist)
 *   · >4 options · domain rules in the control — defaults, persistence, visibility
 *   belong to the tenant
 * @states value-active · hover (inactive segment) · focus · disabled (whole group)
 * @madeof —
 */
export function SegmentedControl({
  options,
  value,
  onChange,
  ariaLabel,
  disabled = false,
}: {
  options: ReadonlyArray<SegmentOption>;
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  disabled?: boolean;
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  const moveFocus = (from: number, delta: number) => {
    const next = from + delta;
    if (next < 0 || next >= options.length) return;
    refs.current[next]?.focus();
  };

  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={`inline-flex overflow-hidden rounded-button border border-hairline ${
        disabled ? "opacity-disabled" : ""
      }`}
    >
      {options.map((option, i) => {
        const active = option.value === value;
        return (
          <div key={option.value} className="flex">
            {i > 0 && <span aria-hidden="true" className="w-px shrink-0 bg-hairline" />}
            <button
              ref={(el) => {
                refs.current[i] = el;
              }}
              type="button"
              role="radio"
              aria-checked={active}
              disabled={disabled}
              tabIndex={active ? 0 : -1}
              onClick={() => {
                if (!active) onChange(option.value);
              }}
              onKeyDown={(e) => {
                if (e.key === "ArrowRight" || e.key === "ArrowDown") {
                  e.preventDefault();
                  moveFocus(i, 1);
                } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
                  e.preventDefault();
                  moveFocus(i, -1);
                }
              }}
              className={`px-chrome-x py-chrome-tight font-ui text-chrome transition-colors focus-visible:outline focus-visible:outline-focus focus-visible:outline-offset-focus-inset focus-visible:outline-ink-mute ${
                active ? "bg-canvas-soft text-ink" : "bg-transparent text-ink-mute hover:text-ink"
              }`}
            >
              {option.label}
            </button>
          </div>
        );
      })}
    </div>
  );
}
