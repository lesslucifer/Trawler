import { HTTP_METHODS } from "./components/MethodLabel";

export type ChipKey = "q" | "host" | "method" | "status" | "type" | "source" | "is";

export interface Chip {
  key: ChipKey;
  value: string;
  negated: boolean;
}

export type Parsed = { chip: Chip } | { fault: string } | { empty: true };

export interface Suggestion {
  kind: "typed" | "key" | "value";
  text: string;
  hint: string;
  key: ChipKey;
  value?: string;
}

type KeyedKey = Exclude<ChipKey, "q">;

export const KEYS: readonly { key: KeyedKey; hint: string }[] = [
  { key: "host", hint: "a host seen, or part of one" },
  { key: "method", hint: "GET, POST, …" },
  { key: "status", hint: "a code like 404, or a class like 4xx" },
  { key: "type", hint: "the response type, like json" },
  { key: "source", hint: "captured, imported or sent" },
  { key: "is", hint: "pending or https" },
];

const WIRE: Record<ChipKey, string> = {
  q: "q",
  host: "host",
  method: "method",
  status: "status",
  type: "content_type",
  source: "source",
  is: "is",
};

type Value = readonly [value: string, hint: string];

const FIXED: Record<"status" | "type" | "source" | "is", readonly Value[]> = {
  status: [
    ["2xx", "success"],
    ["3xx", "redirect"],
    ["4xx", "client error"],
    ["5xx", "server error"],
  ],
  type: ["json", "html", "xml", "javascript", "css", "image", "font", "form", "text"].map((t) => [t, ""] as const),
  source: [
    ["captured", "through the proxy"],
    ["imported", "from a HAR file or a curl command"],
    ["sent", "by Trawler, from a draft"],
  ],
  is: [
    ["pending", "no response yet"],
    ["https", "sent over https"],
  ],
};

const NEAR_MISS_EDITS = 2;

function isKeyed(word: string): word is KeyedKey {
  return KEYS.some((k) => k.key === word);
}

function hintOf(key: KeyedKey): string {
  return KEYS.find((k) => k.key === key)?.hint ?? "";
}

function edits(a: string, b: string): number {
  let previous = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    const row = [i];
    for (let j = 1; j <= b.length; j++) {
      row[j] = Math.min(previous[j] + 1, row[j - 1] + 1, previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    }
    previous = row;
  }
  return previous[b.length];
}

function judge(key: KeyedKey, value: string): { value: string } | { fault: string } {
  switch (key) {
    case "status":
      return /^[1-5](\d\d|xx)$/i.test(value)
        ? { value: value.toLowerCase() }
        : { fault: `“${value}” isn't a status — try a code like 404 or a class like 4xx` };
    case "method":
      return /^[a-z]+$/i.test(value) ? { value: value.toUpperCase() } : { fault: "a method is letters only, like GET" };
    case "source":
    case "is": {
      const allowed = FIXED[key].map(([v]) => v);
      return allowed.includes(value.toLowerCase())
        ? { value: value.toLowerCase() }
        : { fault: `${key} is ${allowed.slice(0, -1).join(", ")} or ${allowed[allowed.length - 1]}` };
    }
    default:
      return { value };
  }
}

function split(draft: string): { negated: boolean; rest: string } {
  const trimmed = draft.trim();
  const negated = trimmed.startsWith("-") && trimmed.length > 1;
  return { negated, rest: negated ? trimmed.slice(1) : trimmed };
}

const KEYED = /^([A-Za-z_]+):(.*)$/s;

export function parseDraft(draft: string): Parsed {
  const { negated, rest } = split(draft);
  if (rest === "") return { empty: true };
  if (rest.startsWith('"')) {
    if (rest.length < 2 || !rest.endsWith('"')) return { fault: "a quote is opened and never closed" };
    const value = rest.slice(1, -1);
    return value.trim() ? { chip: { key: "q", value, negated } } : { fault: "the quotes hold nothing" };
  }
  const keyed = KEYED.exec(rest);
  if (keyed) {
    const [, typedKey, value] = keyed;
    const key = typedKey.toLowerCase();
    if (isKeyed(key)) {
      if (!value) return { fault: `${key}: needs a value — ${hintOf(key)}` };
      const judged = judge(key, value);
      return "fault" in judged ? judged : { chip: { key, value: judged.value, negated } };
    }
    const near = key.length >= 2 && KEYS.find((k) => edits(key, k.key) <= NEAR_MISS_EDITS);
    if (near) return { fault: `unknown key “${typedKey}” — did you mean ${near.key}?` };
  }
  return { chip: { key: "q", value: rest, negated } };
}

export function sameChip(a: Chip, b: Chip): boolean {
  return a.key === b.key && a.value === b.value && a.negated === b.negated;
}

function bare(chip: Chip): string {
  return `${chip.negated ? "-" : ""}${chip.key === "q" ? chip.value : `${chip.key}:${chip.value}`}`;
}

/** The chip as it would be typed: a text chip is quoted when it holds a space or its bare form would read back as something else. */
export function chipLabel(chip: Chip): string {
  const plain = bare(chip);
  if (chip.key !== "q") return plain;
  const reread = parseDraft(plain);
  const readsBack = !/\s/.test(chip.value) && "chip" in reread && sameChip(reread.chip, chip);
  return readsBack ? plain : `${chip.negated ? "-" : ""}"${chip.value}"`;
}

export function hasOpenQuote(draft: string): boolean {
  return (draft.match(/"/g) ?? []).length % 2 === 1;
}

function valuesOf(key: KeyedKey, hosts: readonly string[]): readonly Value[] {
  if (key === "host") return hosts.map((h) => [h, ""] as const);
  if (key === "method") return HTTP_METHODS.map((m) => [m, ""] as const);
  return FIXED[key];
}

export function suggest(draft: string, hosts: readonly string[]): Suggestion[] {
  const start = draft.trimStart();
  const negated = start.startsWith("-");
  const rest = negated ? start.slice(1) : start;
  const sign = negated ? "-" : "";
  const rows: Suggestion[] = [];
  const keyed = KEYED.exec(rest);
  const key = keyed?.[1].toLowerCase();
  if (keyed && key && isKeyed(key)) {
    const typed = keyed[2].toLowerCase();
    for (const [value, hint] of valuesOf(key, hosts)) {
      if (value.toLowerCase().includes(typed)) rows.push({ kind: "value", text: `${sign}${key}:${value}`, hint, key, value });
    }
  } else if (!rest.includes(":") && !rest.startsWith('"')) {
    const typed = rest.toLowerCase();
    for (const k of KEYS) {
      if (k.key.startsWith(typed)) {
        rows.push({ kind: "key", text: `${sign}${k.key}:`, hint: negated ? `leave out by ${k.key}` : k.hint, key: k.key });
      }
    }
  }
  const parsed = parseDraft(draft);
  if ("chip" in parsed) {
    const label = chipLabel(parsed.chip);
    if (!rows.some((row) => row.text.toLowerCase() === label.toLowerCase())) {
      const hint =
        parsed.chip.key !== "q" ? "as typed" : negated ? "leave out every URI containing it" : "search the URI for it";
      rows.unshift({ kind: "typed", text: label, hint, key: parsed.chip.key, value: parsed.chip.value });
    }
  }
  return rows;
}

export function paramOf(chip: Chip): string {
  return `${chip.negated ? "not_" : ""}${WIRE[chip.key]}`;
}

export function chipParams(chips: readonly Chip[]): URLSearchParams {
  const params = new URLSearchParams();
  for (const chip of chips) params.append(paramOf(chip), chip.value);
  return params;
}

/** The core leads every filter refusal with the parameter as sent (DEC-71 rule 6). */
export function refusedParam(reason: string): string {
  return reason.trim().split(/\s/, 1)[0] ?? "";
}

export function readChip(saved: unknown): Chip | null {
  if (typeof saved !== "object" || saved === null) return null;
  const { key, value, negated } = saved as Record<string, unknown>;
  if (typeof key !== "string" || typeof value !== "string" || typeof negated !== "boolean") return null;
  const claimed = { key, value, negated } as Chip;
  if (!(key === "q" || isKeyed(key))) return null;
  const reread = parseDraft(chipLabel(claimed));
  return "chip" in reread && sameChip(reread.chip, claimed) ? claimed : null;
}
