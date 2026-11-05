export interface TimeReading {
  kind: "time";
  local: string;
  zone: string;
  utc: string | null;
  readAs: string;
  relative: string;
  text: string;
  bytes: Uint8Array;
}

export type TimeRead = TimeReading | { kind: "unreadable"; reason: string };

interface Instant {
  seconds: number;
  fraction: string;
  readAs: string;
  dateOnly?: boolean;
}

const UNITS = [
  { digits: [9, 10], scale: 0, name: "seconds" },
  { digits: [12, 13], scale: 3, name: "milliseconds" },
  { digits: [15, 16], scale: 6, name: "microseconds" },
  { digits: [18, 19], scale: 9, name: "nanoseconds" },
];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const ISO_DATE_TIME = String.raw`(?<![\w-])\d{4}-\d{2}-\d{2}(?:[Tt ]\d{2}:\d{2}(?::\d{2}(?:[.,]\d{1,9})?)?(?:[Zz]|[+-]\d{2}:?\d{2})?)?(?![\w-])`;
const HTTP_DATE = String.raw`(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun), \d{1,2}[ -][A-Za-z]{3}[ -]\d{4} \d{2}:\d{2}:\d{2} (?:GMT|UTC)`;
export const TIME_SHAPE = new RegExp(`${ISO_DATE_TIME}|${HTTP_DATE}`, "g");
const TIMESTAMP = /^(\d+)(?:\.(\d+))?$/;
const ISO = /^(\d{4})-(\d{2})-(\d{2})(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:[.,](\d{1,9}))?)?(?:([Zz])|(([+-])(\d{2}):?(\d{2})))?)?$/;
const HTTP = /^(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun), (\d{1,2})[ -]([A-Za-z]{3})[ -](\d{4}) (\d{2}):(\d{2}):(\d{2}) (?:GMT|UTC)$/;

const unreadable = (reason: string): TimeRead => ({ kind: "unreadable", reason });
const pad = (n: number, width = 2) => String(n).padStart(width, "0");
const significant = (fraction: string) => fraction.replace(/0+$/, "");

export const looksLikeTime = (input: string) => {
  const trimmed = input.trim();
  const stamp = TIMESTAMP.exec(trimmed);
  if (stamp) return UNITS.some((unit) => unit.digits.includes(stamp[1].length) && (!stamp[2] || unit.scale === 0));
  return new RegExp(`^(?:${ISO_DATE_TIME}|${HTTP_DATE})$`).test(trimmed);
};

function fromTimestamp(whole: string, fraction: string | undefined): Instant | string {
  const unit = UNITS.find((each) => each.digits.includes(whole.length));
  if (!unit) {
    return `${whole.length} digits is not a Unix time: seconds have 9–10, milliseconds 12–13, microseconds 15–16, nanoseconds 18–19`;
  }
  if (fraction !== undefined && unit.scale > 0) return `a fraction follows only a Unix time in seconds, and this one is in ${unit.name}`;
  const cut = whole.length - unit.scale;
  return {
    seconds: Number(whole.slice(0, cut)),
    fraction: significant(unit.scale > 0 ? whole.slice(cut) : (fraction ?? "")),
    readAs: `Unix time, ${unit.name}`,
  };
}

function civil(year: number, month: number, day: number, hour: number, minute: number, second: number): number | string {
  if (month < 1 || month > 12) return `month ${pad(month)} is not a month`;
  const days = new Date(Date.UTC(year, month, 0)).getUTCDate();
  if (day < 1 || day > days) return `${pad(year, 4)}-${pad(month)}-${pad(day)} is not a day of the calendar`;
  if (hour > 23 || minute > 59 || second > 59) return `${pad(hour)}:${pad(minute)}:${pad(second)} is not a time of day`;
  const at = new Date(0);
  at.setUTCFullYear(year, month - 1, day);
  at.setUTCHours(hour, minute, second);
  return at.getTime() / 1000;
}

function fromIso(match: RegExpExecArray): Instant | string {
  const [, year, month, day, hour, minute, second, fraction, zulu, offsetWritten, sign, offsetHours, offsetMinutes] = match;
  const seconds = civil(+year, +month, +day, +(hour ?? 0), +(minute ?? 0), +(second ?? 0));
  if (typeof seconds === "string") return seconds;
  if (hour === undefined) return { seconds, fraction: "", readAs: "ISO 8601 date", dateOnly: true };
  if (sign && (+offsetHours > 23 || +offsetMinutes > 59)) return `“${offsetWritten}” is not a zone offset`;
  const offset = sign ? (sign === "-" ? -1 : 1) * (+offsetHours * 60 + +offsetMinutes) : 0;
  const readAs = zulu || sign ? "ISO 8601" : "ISO 8601, no zone: read as UTC";
  return { seconds: seconds - offset * 60, fraction: significant(fraction ?? ""), readAs };
}

function fromHttp(match: RegExpExecArray): Instant | string {
  const [, day, monthName, year, hour, minute, second] = match;
  const month = MONTHS.findIndex((name) => name.toLowerCase() === monthName.toLowerCase()) + 1;
  if (month === 0) return `“${monthName}” is not a month`;
  const seconds = civil(+year, month, +day, +hour, +minute, +second);
  return typeof seconds === "string" ? seconds : { seconds, fraction: "", readAs: "HTTP date" };
}

function instantOf(input: string): Instant | string {
  const stamp = TIMESTAMP.exec(input);
  if (stamp) return fromTimestamp(stamp[1], stamp[2]);
  const iso = ISO.exec(input);
  if (iso) return fromIso(iso);
  const http = HTTP.exec(input);
  if (http) return fromHttp(http);
  return "not a Unix time, a date or a date-time";
}

interface WallClock {
  date: string;
  text: string;
  offsetMinutes: number;
}

function wallClock(seconds: number, zone: string, fraction: string): WallClock {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: zone,
    hourCycle: "h23",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).formatToParts(new Date(seconds * 1000));
  const part = (type: Intl.DateTimeFormatPartTypes) => Number(parts.find((each) => each.type === type)!.value);
  const asUtc = Date.UTC(part("year"), part("month") - 1, part("day"), part("hour"), part("minute"), part("second"));
  const date = `${pad(part("year"), 4)}-${pad(part("month"))}-${pad(part("day"))}`;
  const text = `${date} ${pad(part("hour"))}:${pad(part("minute"))}:${pad(part("second"))}${fraction ? `.${fraction}` : ""}`;
  return { date, text, offsetMinutes: Math.round((asUtc - seconds * 1000) / 60000) };
}

function offsetText(minutes: number): string {
  const size = Math.abs(minutes);
  return `${minutes < 0 ? "-" : "+"}${pad(Math.floor(size / 60))}:${pad(size % 60)}`;
}

const RELATIVE_STEPS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["second", 60],
  ["minute", 60],
  ["hour", 24],
  ["day", 30.4375],
  ["month", 12],
  ["year", Infinity],
];

const DAY = 86400;

function relative(seconds: number, now: number): string {
  let amount = seconds - now / 1000;
  for (const [unit, next] of RELATIVE_STEPS) {
    const rounded = Math.round(amount);
    if (Math.abs(rounded) < next) return new Intl.RelativeTimeFormat("en", { numeric: "auto" }).format(rounded, unit);
    amount /= next;
  }
  return "";
}

function relativeDays(days: number): string {
  if (Math.abs(days) < RELATIVE_STEPS[3][1]) return new Intl.RelativeTimeFormat("en", { numeric: "auto" }).format(days, "day");
  return relative(days * DAY, 0);
}

export const systemZone = () => Intl.DateTimeFormat().resolvedOptions().timeZone;

const reading = (shown: string, utc: string | null, zone: string, readAs: string, relativeText: string): TimeReading => ({
  kind: "time",
  local: shown,
  zone,
  utc,
  readAs,
  relative: relativeText,
  text: shown,
  bytes: new TextEncoder().encode(shown),
});

// A date names a calendar day, not an instant: it is shown as written, never
// moved across zones, and counted in days from today in the user's zone.
function dateReading({ seconds, readAs }: Instant, zone: string, now: number): TimeReading {
  const weekday = new Intl.DateTimeFormat("en-US", { timeZone: "UTC", weekday: "long" }).format(new Date(seconds * 1000));
  const today = Date.parse(`${wallClock(Math.floor(now / 1000), zone, "").date}T00:00:00Z`) / 1000;
  const shown = `${wallClock(seconds, "UTC", "").date}, ${weekday}`;
  return reading(shown, null, zone, readAs, relativeDays(Math.round((seconds - today) / DAY)));
}

export function time(input: string, zone = systemZone(), now = Date.now()): TimeRead {
  const instant = instantOf(input.trim());
  if (typeof instant === "string") return unreadable(instant);
  if (instant.dateOnly) return dateReading(instant, zone, now);
  const { seconds, fraction, readAs } = instant;
  const local = wallClock(seconds, zone, fraction);
  const localText = `${local.text} ${offsetText(local.offsetMinutes)}`;
  return reading(localText, `${wallClock(seconds, "UTC", fraction).text}Z`, zone, readAs, relative(seconds, now));
}
