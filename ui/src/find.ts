import type { TextRange } from "./components/index";

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
const HEX_PAIRS = /^\s*(?:[0-9a-fA-F]{2}\s*)+$/;

export function findMatches(text: string, query: string, limit = MATCH_LIMIT): Found {
  if (!query) return { ranges: [], capped: false };
  const pattern = new RegExp(query.replace(SPECIAL, "\\$&"), "gi");
  const ranges: TextRange[] = [];
  for (let found = pattern.exec(text); found; found = pattern.exec(text)) {
    if (ranges.length === limit) return { ranges, capped: true };
    ranges.push({ from: found.index, to: found.index + found[0].length });
  }
  return { ranges, capped: false };
}

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
