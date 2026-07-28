import { useEffect, useState } from "react";
import moment from "moment";

const RELATIVE_TICK_MS = 10_000;

/**
 * TimestampText — point-in-time text; local wall-clock, absolute HH:MM:SS by default.
 * @tier T1 — time parsing/formatting only (moment), no business logic
 * @props timestamp: string | number | Date | null — anything moment parses, passed raw; format?: moment format string or "relative"
 * @anatomy muted mono clock text; default 24 h HH:MM:SS with cross-day prefix "DD/MM · " when date ≠ today (year appended when ≠ current); tabular-nums, fixed-width
 * @modes default (same-day/cross-day logic) · format="<moment tokens>" (custom — ms, 12 h, any layout) · format="relative" (fromNow, 10 s tick, opt-in only)
 * @use list rows (time column) · flow detail overview; anywhere a point-in-time label reads against logs
 * @avoid relative as a list-column default (log correlation is absolute) · UTC — always local in every mode · caller-side pre-formatting — pass the raw value
 * @states same-day HH:MM:SS (S1) · cross-day "DD/MM · HH:MM:SS" (S2) · ms truncated by default (S3, floor) · absent/unparseable → em-dash (S4) · custom format (S5) · relative (S6)
 * @madeof —
 */
export function TimestampText({
  timestamp,
  format,
}: {
  timestamp: string | number | Date | null;
  format?: string;
}) {
  const [, tick] = useState(0);
  const relative = format === "relative";

  useEffect(() => {
    if (!relative) return;
    const id = setInterval(() => tick((n) => n + 1), RELATIVE_TICK_MS);
    return () => clearInterval(id);
  }, [relative]);

  const className = "whitespace-nowrap font-mono text-body tabular-nums text-ink-mute";
  const at = timestamp === null ? null : moment(timestamp);

  if (!at?.isValid()) return <span className={className}>—</span>;

  return <span className={className}>{render(at, format)}</span>;
}

function render(at: moment.Moment, format?: string): string {
  if (format === "relative") return at.fromNow();
  if (format) return at.format(format);

  const clock = at.format("HH:mm:ss");
  if (at.isSame(moment(), "day")) return clock;

  const datePart = at.isSame(moment(), "year") ? at.format("DD/MM") : at.format("DD/MM/YYYY");
  return `${datePart} · ${clock}`;
}
