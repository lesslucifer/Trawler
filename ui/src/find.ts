import type { TextRange } from "./components/index";
import { HEX_PAIRS, type FindChip } from "./findGrammar";
import type { JsonNode } from "./jsonTools";

export const MATCH_LIMIT = 10_000;

export interface Found {
  ranges: TextRange[];
  capped: boolean;
}

export type HexQueryMode = "bytes" | "text";

export interface HexQuery {
  mode: HexQueryMode;
  needle: Uint8Array;
}

const SPECIAL = /[.*+?^${}()|[\]\\]/g;

export function readHexQuery(query: string): HexQuery {
  if (HEX_PAIRS.test(query)) {
    const digits = query.replace(/\s+/g, "");
    const needle = new Uint8Array(digits.length / 2);
    for (let i = 0; i < needle.length; i += 1) needle[i] = parseInt(digits.slice(i * 2, i * 2 + 2), 16);
    return { mode: "bytes", needle };
  }
  return { mode: "text", needle: new TextEncoder().encode(query) };
}

const foldAscii = (byte: number) => (byte >= 0x41 && byte <= 0x5a ? byte + 0x20 : byte);

export function findBytes(bytes: Uint8Array, query: HexQuery, limit = MATCH_LIMIT): Found {
  const { needle } = query;
  const ranges: TextRange[] = [];
  if (needle.length === 0) return { ranges, capped: false };
  const fold = query.mode === "text" ? foldAscii : (byte: number) => byte;
  const wanted = needle.map(fold);
  const last = bytes.length - wanted.length;
  for (let at = 0; at <= last; at += 1) {
    let j = 0;
    while (j < wanted.length && fold(bytes[at + j]) === wanted[j]) j += 1;
    if (j < wanted.length) continue;
    if (ranges.length === limit) return { ranges, capped: true };
    ranges.push({ from: at, to: at + wanted.length });
    at += wanted.length - 1;
  }
  return { ranges, capped: false };
}

export type Scan = (text: string, limit: number) => Found;

const literal = (value: string) => value.replace(SPECIAL, "\\$&");

function scanFor(source: string, flags: string): Scan {
  let pattern: RegExp;
  try {
    pattern = new RegExp(source, flags);
  } catch {
    return () => ({ ranges: [], capped: false });
  }
  return (text, limit) => {
    pattern.lastIndex = 0;
    const ranges: TextRange[] = [];
    for (let found = pattern.exec(text); found; found = pattern.exec(text)) {
      if (found[0] === "") {
        pattern.lastIndex += 1;
        continue;
      }
      if (ranges.length === limit) return { ranges, capped: true };
      ranges.push({ from: found.index, to: found.index + found[0].length });
    }
    return { ranges, capped: false };
  };
}

const hex4 = (unit: number) => unit.toString(16).padStart(4, "0");

function jsonSpellings(value: string): string {
  return value.split("").map((char) => {
    const spellings = new Set([literal(JSON.stringify(char).slice(1, -1)), literal(char)]);
    for (let i = 0; i < char.length; i += 1) {
      const unit = char.charCodeAt(i);
      if (unit >= 0x7f || unit < 0x20) {
        spellings.add(`\\\\u${hex4(unit).toLowerCase()}`);
        spellings.add(`\\\\u${hex4(unit).toUpperCase()}`);
      }
    }
    if (char === "/") spellings.add("\\\\/");
    return spellings.size === 1 ? [...spellings][0] : `(?:${[...spellings].join("|")})`;
  }).join("");
}

export function scanOf(chip: FindChip, jsonEscaped = false): Scan | null {
  const needle = jsonEscaped ? jsonSpellings(chip.value) : literal(chip.value);
  switch (chip.key) {
    case "re":
      return scanFor(chip.value, "gi");
    case "case":
      return scanFor(needle, "g");
    case "plain":
    case "key":
    case "value":
      return scanFor(needle, "gi");
    default:
      return null;
  }
}

export function mergeRanges(lists: readonly Found[], limit = MATCH_LIMIT): Found {
  const sorted = lists.flatMap((list) => list.ranges).sort((a, b) => a.from - b.from || b.to - a.to);
  const merged: TextRange[] = [];
  for (const range of sorted) {
    const last = merged[merged.length - 1];
    if (last && range.from < last.to) last.to = Math.max(last.to, range.to);
    else merged.push({ ...range });
  }
  const over = merged.length > limit;
  return { ranges: over ? merged.slice(0, limit) : merged, capped: over || lists.some((list) => list.capped) };
}

export interface Structure {
  keys: TextRange[];
  values: TextRange[];
  jsonEscaped: boolean;
}

export function jsonStructure(root: JsonNode): Structure {
  const keys: TextRange[] = [];
  const values: TextRange[] = [];
  const walk = (node: JsonNode) => {
    if (node.type === "object") {
      for (const member of node.members) {
        keys.push({ from: member.from + 1, to: member.from + member.keyRaw.length - 1 });
        walk(member.node);
      }
    } else if (node.type === "array") node.items.forEach(walk);
    else if (node.type === "string") values.push({ from: node.from + 1, to: node.to - 1 });
    else values.push({ from: node.from, to: node.to });
  };
  walk(root);
  return { keys, values, jsonEscaped: true };
}

export function structureNames(structure: Structure, text: string, decodeForm: boolean, limit = 500): string[] {
  const names = new Set<string>();
  for (const { from, to } of structure.keys) {
    const raw = text.slice(from, to);
    try {
      names.add(structure.jsonEscaped ? (JSON.parse(`"${raw}"`) as string) : decodeForm ? decodeURIComponent(raw.replace(/\+/g, " ")) : raw);
    } catch {
      names.add(raw);
    }
    if (names.size >= limit) break;
  }
  return [...names];
}

function within(scan: Scan, text: string, parts: readonly TextRange[], limit: number): Found {
  const ranges: TextRange[] = [];
  for (const part of parts) {
    const found = scan(text.slice(part.from, part.to), limit - ranges.length);
    for (const range of found.ranges) ranges.push({ from: part.from + range.from, to: part.from + range.to });
    if (found.capped) return { ranges, capped: true };
  }
  return { ranges, capped: false };
}

export function markText(text: string, chips: readonly FindChip[], structure: Structure | null, limit = MATCH_LIMIT): Found {
  const lists: Found[] = [];
  for (const chip of chips) {
    if (chip.key === "key" || chip.key === "value") {
      const scan = structure && scanOf(chip, structure.jsonEscaped);
      if (scan && structure) lists.push(within(scan, text, chip.key === "key" ? structure.keys : structure.values, limit));
    } else {
      const scan = scanOf(chip);
      if (scan) lists.push(scan(text, limit));
    }
  }
  return mergeRanges(lists, limit);
}

export function hexQueryOf(chip: FindChip): HexQuery | null {
  switch (chip.key) {
    case "bytes":
      return HEX_PAIRS.test(chip.value) ? readHexQuery(chip.value) : null;
    case "text":
      return { mode: "text", needle: new TextEncoder().encode(chip.value) };
    case "plain":
      return readHexQuery(chip.value);
    default:
      return null;
  }
}

export function markBytes(bytes: Uint8Array, chips: readonly FindChip[], limit = MATCH_LIMIT): Found & { as: HexQueryMode | null } {
  const lists: Found[] = [];
  let as: HexQueryMode | null = null;
  for (const chip of chips) {
    const query = hexQueryOf(chip);
    if (!query) continue;
    if (chip.key === "plain") as = query.mode;
    lists.push(findBytes(bytes, query, limit));
  }
  return { ...mergeRanges(lists, limit), as };
}
