import { mergeRanges, MATCH_LIMIT, scanOf } from "./find";
import type { FindChip } from "./findGrammar";
import type { TextRange } from "./components/index";

export interface FormPair {
  index: number;
  raw: string;
  key: string;
  value: string;
  kept: boolean;
}

export type PairPart = "key" | "value";

function decodePart(part: string): { text: string; kept: boolean } {
  try {
    return { text: decodeURIComponent(part.replace(/\+/g, " ")), kept: false };
  } catch {
    return { text: part, kept: true };
  }
}

const raws = (text: string) => (text === "" ? [] : text.split("&"));

export function parsePairs(text: string): FormPair[] {
  return raws(text).map((raw, index) => {
    const [rawKey, rawValue] = splitRaw(raw);
    const key = decodePart(rawKey);
    const value = decodePart(rawValue);
    return { index, raw, key: key.text, value: value.text, kept: key.kept || value.kept };
  });
}

function splitRaw(raw: string): [key: string, value: string] {
  const eq = raw.indexOf("=");
  return eq < 0 ? [raw, ""] : [raw.slice(0, eq), raw.slice(eq + 1)];
}

const LONE_SURROGATE = /[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/g;

const encodePart = (text: string) => encodeURIComponent(text.replace(LONE_SURROGATE, "\ufffd"));

export function editPair(text: string, index: number, part: PairPart, edited: string): string {
  return raws(text)
    .map((raw, i) => {
      if (i !== index) return raw;
      const [key, value] = splitRaw(raw);
      if (part === "value") return `${key}=${encodePart(edited)}`;
      return raw.includes("=") ? `${encodePart(edited)}=${value}` : encodePart(edited);
    })
    .join("&");
}

export function addPairAfter(text: string, index: number): string {
  const next = raws(text);
  next.splice(index + 1, 0, "=");
  return next.join("&");
}

export function removePair(text: string, index: number): string {
  return raws(text)
    .filter((_, i) => i !== index)
    .join("&");
}

export interface Repeat {
  nth: number;
  of: number;
}

export function repeats(pairs: FormPair[]): Map<number, Repeat> {
  const totals = new Map<string, number>();
  for (const pair of pairs) totals.set(pair.key, (totals.get(pair.key) ?? 0) + 1);
  const seen = new Map<string, number>();
  const out = new Map<number, Repeat>();
  for (const pair of pairs) {
    const of = totals.get(pair.key) ?? 1;
    const nth = (seen.get(pair.key) ?? 0) + 1;
    seen.set(pair.key, nth);
    if (of > 1) out.set(pair.index, { nth, of });
  }
  return out;
}

export const SORT_BY_KEY_STEP = "sorted by key";

export function sortByKey(text: string): string {
  return parsePairs(text)
    .sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0))
    .map((pair) => pair.raw)
    .join("&");
}

export function pairsAsJson(pairs: FormPair[]): string {
  const byKey = new Map<string, string[]>();
  for (const pair of pairs) {
    const values = byKey.get(pair.key);
    if (values) values.push(pair.value);
    else byKey.set(pair.key, [pair.value]);
  }
  if (byKey.size === 0) return "{}";
  const members = [...byKey].map(([key, values]) => {
    const value = JSON.stringify(values.length === 1 ? values[0] : values, null, 2).replace(/\n/g, "\n  ");
    return `  ${JSON.stringify(key)}: ${value}`;
  });
  return `{\n${members.join(",\n")}\n}`;
}

export interface PairMatches {
  first: number;
  key: TextRange[];
  value: TextRange[];
}

export interface PairsFound {
  byPair: Map<number, PairMatches>;
  total: number;
  capped: boolean;
}

const PART_KEYS: Record<PairPart, readonly FindChip["key"][]> = {
  key: ["plain", "re", "case", "key"],
  value: ["plain", "re", "case", "value"],
};

function markPart(text: string, part: PairPart, chips: readonly FindChip[], limit: number) {
  const lists = chips.flatMap((chip) => {
    const scan = PART_KEYS[part].includes(chip.key) ? scanOf(chip) : null;
    return scan ? [scan(text, limit)] : [];
  });
  return mergeRanges(lists, limit);
}

export function markPairs(pairs: FormPair[], chips: readonly FindChip[], limit = MATCH_LIMIT): PairsFound {
  const byPair = new Map<number, PairMatches>();
  let total = 0;
  if (chips.length === 0) return { byPair, total, capped: false };
  for (const pair of pairs) {
    const key = markPart(pair.key, "key", chips, limit - total);
    const value = markPart(pair.value, "value", chips, limit - total - key.ranges.length);
    const found = key.ranges.length + value.ranges.length;
    if (found) byPair.set(pair.index, { first: total, key: key.ranges, value: value.ranges });
    total += found;
    if (key.capped || value.capped || total >= limit) return { byPair, total, capped: key.capped || value.capped };
  }
  return { byPair, total, capped: false };
}

export function findInPairs(pairs: FormPair[], query: string, limit = MATCH_LIMIT): PairsFound {
  return markPairs(pairs, query ? [{ key: "plain", value: query }] : [], limit);
}
