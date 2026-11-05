import type { DataKind } from "./dataKind";
import { edits } from "./query";

export type FindKey = "plain" | "key" | "value" | "re" | "case" | "bytes" | "text";

export interface FindChip {
  key: FindKey;
  value: string;
}

export type FoundDraft = { chip: FindChip } | { fault: string } | { empty: true };

export interface FindSuggestion {
  kind: "key" | "value";
  text: string;
  hint: string;
}

type Keyed = Exclude<FindKey, "plain">;

const HINTS: Record<Keyed, string> = {
  key: "only inside keys",
  value: "only inside values",
  re: "a regular expression",
  case: "match case",
  bytes: "hex pairs, like 89 50 4e",
  text: "ASCII",
};

const NAMED = Object.keys(HINTS) as Keyed[];
const NEAR_MISS_EDITS = 2;
export const HEX_PAIRS = /^\s*(?:[0-9a-fA-F]{2}\s*)+$/;
const KEYED = /^([A-Za-z_]+):(.*)$/s;
const NAMES_SHOWN = 12;

export const NOTHING_TO_EXCLUDE = "find marks what it finds; there is nothing to exclude, so - has no meaning here (quote it to find a dash)";

export function keysFor(kind: DataKind): Keyed[] {
  if (kind === "json" || kind === "form") return ["key", "value", "re", "case"];
  if (kind === "hex") return ["bytes", "text"];
  return ["re", "case"];
}

const isKeyed = (word: string): word is Keyed => (NAMED as string[]).includes(word);

const unquote = (value: string) => (value.length >= 2 && value.startsWith('"') && value.endsWith('"') ? value.slice(1, -1) : value);

const quoted = (value: string) => (/\s/.test(value) ? `"${value}"` : value);

export function hasOpenQuote(draft: string): boolean {
  return (draft.match(/"/g) ?? []).length % 2 === 1;
}

function closest(word: string): Keyed | undefined {
  const ranked = NAMED.map((key) => ({ key, away: edits(word, key) })).sort((a, b) => a.away - b.away);
  return ranked[0].away <= NEAR_MISS_EDITS ? ranked[0].key : undefined;
}

export function parseFind(draft: string): FoundDraft {
  const text = draft.trim();
  if (text === "") return { empty: true };
  if (text.startsWith("-") && text.length > 1) return { fault: NOTHING_TO_EXCLUDE };
  if (text.startsWith('"')) {
    if (text.length < 2 || !text.endsWith('"')) return { fault: "a quote is opened and never closed" };
    const value = text.slice(1, -1);
    return value.trim() ? { chip: { key: "plain", value } } : { fault: "the quotes hold nothing" };
  }
  const keyed = KEYED.exec(text);
  if (keyed) {
    const [, typed, rest] = keyed;
    const key = typed.toLowerCase();
    if (isKeyed(key)) {
      if (rest.startsWith('"') && !(rest.length >= 2 && rest.endsWith('"'))) return { fault: "a quote is opened and never closed" };
      const value = unquote(rest);
      if (!value) return { fault: `${key}: needs something to find after it — ${HINTS[key]}` };
      if (key === "re") {
        try {
          new RegExp(value);
        } catch (error) {
          return { fault: `not a regular expression: ${(error as Error).message.replace(/^Invalid regular expression: /, "")}` };
        }
      }
      if (key === "bytes" && !HEX_PAIRS.test(value)) return { fault: "bytes: takes hex pairs, like 89 50 4e" };
      return { chip: { key, value } };
    }
    const near = key.length >= 2 && closest(key);
    if (near) return { fault: `no key “${typed}:” — did you mean ${near}:?` };
  }
  return { chip: { key: "plain", value: text } };
}

export function findLabel(chip: FindChip): string {
  if (chip.key !== "plain") return `${chip.key}:${quoted(chip.value)}`;
  const reread = parseFind(chip.value);
  const readsBack = "chip" in reread && reread.chip.key === "plain" && reread.chip.value === chip.value && !/\s/.test(chip.value);
  return readsBack ? chip.value : `"${chip.value}"`;
}

export function suggestFind(draft: string, kind: DataKind, names: readonly string[]): FindSuggestion[] {
  const text = draft.trim();
  if (text === "" || text.startsWith('"')) return [];
  const keys = keysFor(kind);
  const keyed = KEYED.exec(text);
  if (keyed) {
    if (keyed[1].toLowerCase() !== "key" || !keys.includes("key")) return [];
    const typed = keyed[2].toLowerCase();
    return names
      .filter((name) => !name.includes('"') && name.toLowerCase().includes(typed) && name.toLowerCase() !== typed)
      .slice(0, NAMES_SHOWN)
      .map((name) => ({ kind: "value", text: `key:${quoted(name)}`, hint: "a key in this document" }));
  }
  if (text.includes(":")) return [];
  const typed = text.toLowerCase();
  return keys.filter((key) => key.startsWith(typed)).map((key) => ({ kind: "key", text: `${key}:`, hint: HINTS[key] }));
}

export function refusedReason(chip: FindChip, kind: DataKind): string | null {
  switch (chip.key) {
    case "key":
    case "value":
      return kind === "json" || kind === "form" ? null : `${chip.key}: needs JSON or form; this is ${kind}`;
    case "re":
    case "case":
      return kind === "hex" || kind === "image" ? `${chip.key}: needs text; this is ${kind}` : null;
    case "bytes":
    case "text":
      return kind === "hex" ? null : `${chip.key}: needs hex; this is ${kind}`;
    default:
      return null;
  }
}
