import { useEffect, useState } from "react";
import moment from "moment";

const RELATIVE_TICK_MS = 10_000;

const ABSENT = "—";

// A virtualized list re-renders its whole window on every debounced refresh, so
// the same handful of timestamps are formatted over and over. Nothing evicts a
// row that scrolls away, so the cap bounds how many distinct timestamps are
// remembered between resets — not how many are on screen.
const MEMO_MAX = 4096;

/**
 * TimestampText — point-in-time text; local wall-clock, absolute HH:MM:SS by default.
 * @tier T1 — time parsing/formatting only (moment), no business logic
 * @props timestamp: string | number | Date | null — anything moment parses, passed raw; format?: moment format string or "relative"
 * @anatomy muted mono clock text; default 24 h HH:MM:SS with cross-day prefix "DD/MM · " when date ≠ today (year appended when ≠ current); tabular-nums, fixed-width · relative takes the sentence's own font and ink: it reads as words, not as a clock
 * @modes default (same-day/cross-day logic) · format="<moment tokens>" (custom — ms, 12 h, any layout) · format="relative" (fromNow, 10 s tick, opt-in only, set inside a sentence)
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

  const className = relative ? "whitespace-nowrap" : "whitespace-nowrap font-mono text-body tabular-nums text-ink-mute";

  return <span className={className}>{text(timestamp, format)}</span>;
}

function text(timestamp: string | number | Date | null, format?: string): string {
  if (timestamp === null) return ABSENT;

  // Relative text is the one mode whose answer changes while its input does not,
  // so it is the one mode that may never be remembered.
  if (format === "relative") {
    const at = moment(timestamp);
    return at.isValid() ? at.fromNow() : ABSENT;
  }

  const today = day();
  // A Date and a number are keyed alike because moment reads them alike, but a
  // numeric string is not: moment("1699999999999") is invalid where the number
  // is a real instant, so the two must never meet on one key.
  const value = timestamp instanceof Date ? timestamp.getTime() : timestamp;
  const key = `${format ?? ""}\u0000${typeof value}\u0000${value}`;

  const remembered = cache(today.start).get(key);
  if (remembered !== undefined) return remembered;

  const at = moment(timestamp);
  const result = at.isValid() ? render(at, today, format) : ABSENT;
  remember(today.start, key, result);
  return result;
}

function render(at: moment.Moment, today: Day, format?: string): string {
  if (format) return at.format(format);

  const clock = at.format("HH:mm:ss");
  const ms = at.valueOf();
  if (ms >= today.start && ms < today.end) return clock;

  const datePart =
    ms >= today.yearStart && ms < today.yearEnd ? at.format("DD/MM") : at.format("DD/MM/YYYY");
  return `${datePart} · ${clock}`;
}

// Today's local bounds answer both the day test and the year test by comparison
// rather than by building a moment to compare against, and move only when the
// wall clock leaves them.
interface Day {
  start: number;
  end: number;
  yearStart: number;
  yearEnd: number;
}

let today: Day | null = null;

function day(): Day {
  const now = Date.now();
  if (today !== null && now >= today.start && now < today.end) return today;

  const at = new Date(now);
  const year = at.getFullYear();
  today = {
    start: new Date(year, at.getMonth(), at.getDate()).getTime(),
    end: new Date(year, at.getMonth(), at.getDate() + 1).getTime(),
    yearStart: new Date(year, 0, 1).getTime(),
    yearEnd: new Date(year + 1, 0, 1).getTime(),
  };
  return today;
}

// Every remembered string was rendered against one day's bounds, so the day it
// was rendered under is the generation: crossing midnight abandons the lot
// rather than serving yesterday's "today".
let remembered = new Map<string, string>();
let generation = 0;

function cache(dayStart: number): Map<string, string> {
  if (generation !== dayStart) {
    remembered = new Map();
    generation = dayStart;
  }
  return remembered;
}

// The cap is checked here and never on the way in: a lookup that enforced it
// would throw away the entry it was about to find, and would do so on the render
// that most needs it.
function remember(dayStart: number, key: string, value: string): void {
  const store = cache(dayStart);
  if (store.size >= MEMO_MAX) store.clear();
  store.set(key, value);
}
