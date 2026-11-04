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

export function editPair(text: string, index: number, part: PairPart, edited: string): string {
  return raws(text)
    .map((raw, i) => {
      if (i !== index) return raw;
      const [key, value] = splitRaw(raw);
      if (part === "value") return `${key}=${encodeURIComponent(edited)}`;
      return raw.includes("=") ? `${encodeURIComponent(edited)}=${value}` : encodeURIComponent(edited);
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
