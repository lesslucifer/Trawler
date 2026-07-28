export type DurationUnit = "ms" | "s" | "min" | "h" | "d";

const TO_MS: Record<DurationUnit, number> = {
  ms: 1,
  s: 1000,
  min: 60_000,
  h: 3_600_000,
  d: 86_400_000,
};

const PENDING_PLACEHOLDER = "000 ms";

const SPOKEN: Record<string, string> = {
  ms: "milliseconds",
  s: "seconds",
  min: "minutes",
  h: "hours",
  d: "days",
};

function format(ms: number): { text: string; value: string; unit: string } {
  const scale = (divisor: number, unit: string) => ({
    text: `${(ms / divisor).toFixed(1)} ${unit}`,
    value: (ms / divisor).toFixed(1),
    unit,
  });
  if (ms < TO_MS.s) return { text: `${ms} ms`, value: String(ms), unit: "ms" };
  if (ms < TO_MS.min) return scale(TO_MS.s, "s");
  if (ms < TO_MS.h) return scale(TO_MS.min, "min");
  if (ms < TO_MS.d) return scale(TO_MS.h, "h");
  return scale(TO_MS.d, "d");
}

/**
 * DurationText — a flow's elapsed time; formats a raw duration (unit prop, default ms) to a human-readable string.
 * @tier T1 — duration→human formatting only, no business logic
 * @anatomy right-aligned muted numeric text, tabular-nums, mono · input normalized to ms (unit prop, default "ms") · unit scales: integer ms below 1 s, one decimal in the largest unit ≥ 1 above (s / min / h / d) · invisible fixed-width placeholder while in-flight
 * @use list rows (duration column) · the detail header identity bar
 * @avoid a live-ticking counter while in-flight (hold blank, not a ticker) · tinting fast/slow with ok/warn (that is a T2 concern if ever needed)
 * @states fast (<1 000 ms, integer ms) · slow (≥1 000 ms, one-decimal s/min/h/d) · pending (value=null, invisible placeholder holds column width)
 * @madeof —
 */
export function DurationText({
  value,
  unit = "ms",
}: {
  value: number | null;
  unit?: DurationUnit;
}) {
  const base = "whitespace-nowrap text-right font-mono text-body tabular-nums text-ink-mute";

  if (value === null) {
    return (
      <span aria-hidden="true" className={`${base} select-none text-transparent`}>
        {PENDING_PLACEHOLDER}
      </span>
    );
  }

  const { text, value: shown, unit: shownUnit } = format(value * TO_MS[unit]);

  return (
    <span className={base} aria-label={`${shown} ${SPOKEN[shownUnit]}`}>
      {text}
    </span>
  );
}
