import { HEADERS_LABEL, normalizeLabel, URI_LABEL } from "./contentKind";
import { mergeRanges, MATCH_LIMIT, scanOf, type Scan, type Structure } from "./find";
import type { FindChip } from "./findGrammar";
import type { TextRange } from "./components/index";

export interface FormPair {
  index: number;
  raw: string;
  key: string;
  value: string;
  kept: boolean;
  part?: true;
}

export type PairPart = "key" | "value";

export type FormShape = "query" | "url" | "headers" | "none";

export interface FormRead {
  parts: FormPair[];
  pairs: FormPair[];
}

const FIELD_NAME = /^:?[!#$%&'*+.^_`|~0-9A-Za-z-]+:/;
const ONE_FIELD = /^:?[!#$%&'*+.^_`|~0-9A-Za-z-]+:[ \t]/;
const FOLD = /^[ \t]/;
const URL_START = /^(?:[a-z][a-z0-9+.-]*:\/\/|\/)/i;
const NEVER_RAW_IN_A_FORM = /["<>]/;

const LABEL_SHAPES: Record<string, FormShape> = {
  "application/x-www-form-urlencoded": "query",
  [HEADERS_LABEL]: "headers",
  [URI_LABEL]: "url",
};

export const labelShape = (label: string | null | undefined): FormShape | null => LABEL_SHAPES[normalizeLabel(label) ?? ""] ?? null;

export function formShape(text: string): FormShape {
  const body = text.replace(/\r?\n$/, "");
  const lines = body.split(/\r?\n/);
  if (lines.length === 1) {
    if (URL_START.test(body) && !/\s/.test(body)) return "url";
    if (ONE_FIELD.test(body)) return "headers";
    return NEVER_RAW_IN_A_FORM.test(body) ? "none" : "query";
  }
  return lines.every((line, i) => FIELD_NAME.test(line) || (i > 0 && FOLD.test(line))) ? "headers" : "none";
}

function decodePart(part: string, plusIsSpace = true): { text: string; kept: boolean } {
  try {
    return { text: decodeURIComponent(plusIsSpace ? part.replace(/\+/g, " ") : part), kept: false };
  } catch {
    return { text: part, kept: true };
  }
}

interface Frame {
  head: string;
  opener: string;
  raws: string[];
  join: string;
  tail: string;
}

const ampersandRaws = (text: string) => (text === "" ? [] : text.split("&"));

function headerRaws(body: string, lineBreak: string): string[] {
  const raws: string[] = [];
  for (const line of body === "" ? [] : body.split(lineBreak)) {
    if (raws.length > 0 && FOLD.test(line)) raws[raws.length - 1] += lineBreak + line;
    else raws.push(line);
  }
  return raws;
}

function frameOf(text: string, shape: FormShape): Frame {
  if (shape === "headers") {
    const lineBreak = text.includes("\r\n") ? "\r\n" : "\n";
    const tail = text.endsWith(lineBreak) ? lineBreak : "";
    return { head: "", opener: "", raws: headerRaws(text.slice(0, text.length - tail.length), lineBreak), join: lineBreak, tail };
  }
  if (shape === "url") {
    const hash = text.indexOf("#");
    const beforeHash = hash < 0 ? text : text.slice(0, hash);
    const question = beforeHash.indexOf("?");
    return {
      head: question < 0 ? beforeHash : beforeHash.slice(0, question),
      opener: "?",
      raws: ampersandRaws(question < 0 ? "" : beforeHash.slice(question + 1)),
      join: "&",
      tail: hash < 0 ? "" : text.slice(hash),
    };
  }
  return { head: "", opener: "", raws: ampersandRaws(text), join: "&", tail: "" };
}

const rebuild = ({ head, opener, join, tail }: Frame, raws: string[]) =>
  head + (raws.length > 0 ? opener + raws.join(join) : "") + tail;

function splitRaw(raw: string): [key: string, value: string] {
  const eq = raw.indexOf("=");
  return eq < 0 ? [raw, ""] : [raw.slice(0, eq), raw.slice(eq + 1)];
}

function splitField(raw: string): [name: string, rest?: string] {
  const colon = raw.startsWith(":") && raw.indexOf(":", 1) > 0 ? raw.indexOf(":", 1) : raw.indexOf(":");
  return colon < 0 ? [raw] : [raw.slice(0, colon), raw.slice(colon + 1)];
}

function pairOf(raw: string, index: number, shape: FormShape): FormPair {
  if (shape === "headers") {
    const [name, rest = ""] = splitField(raw.replace(/\r?\n[ \t]+/g, " "));
    return { index, raw, key: name, value: rest.trim(), kept: false };
  }
  const [rawKey, rawValue] = splitRaw(raw);
  const key = decodePart(rawKey);
  const value = decodePart(rawValue);
  return { index, raw, key: key.text, value: value.text, kept: key.kept || value.kept };
}

type Labelled = [label: string, raw: string, decoded: boolean];

const URL_HEAD = /^(?:([a-z][a-z0-9+.-]*):)?(?:\/\/([^/]*))?(.*)$/is;

function authorityParts(authority: string): Labelled[] {
  const at = authority.lastIndexOf("@");
  const hostPort = authority.slice(at + 1);
  const portColon = hostPort.startsWith("[") ? hostPort.indexOf(":", hostPort.indexOf("]")) : hostPort.lastIndexOf(":");
  return [
    ["user", at < 0 ? "" : authority.slice(0, at), true],
    ["host", portColon < 0 ? hostPort : hostPort.slice(0, portColon), false],
    ["port", portColon < 0 ? "" : hostPort.slice(portColon + 1), false],
  ];
}

function urlParts(head: string, tail: string): FormPair[] {
  const [, scheme = "", authority, path] = URL_HEAD.exec(head)!;
  const present = (
    [
      ["scheme", scheme, false],
      ...(authority === undefined ? [] : authorityParts(authority)),
      ["path", path, true],
      ["fragment", tail.slice(1), true],
    ] as Labelled[]
  ).filter(([, raw]) => raw !== "");
  return present.map(([label, raw, decoded], i) => {
    const read = decoded ? decodePart(raw, false) : { text: raw, kept: false };
    return { index: i - present.length, raw, key: label, value: read.text, kept: read.kept, part: true };
  });
}

export function readForm(text: string, shape: FormShape): FormRead {
  if (shape === "none") return { parts: [], pairs: [] };
  const frame = frameOf(text, shape);
  return {
    parts: shape === "url" ? urlParts(frame.head, frame.tail) : [],
    pairs: frame.raws.map((raw, index) => pairOf(raw, index, shape)),
  };
}

export function parsePairs(text: string, shape: FormShape = "query"): FormPair[] {
  return readForm(text, shape).pairs;
}

export function formStructure(text: string, shape: FormShape = "query"): Structure {
  const keys: TextRange[] = [];
  const values: TextRange[] = [];
  if (shape === "none") return { keys, values, jsonEscaped: false };
  const frame = frameOf(text, shape);
  let at = frame.head.length + (frame.raws.length > 0 ? frame.opener.length : 0);
  for (const raw of frame.raws) {
    if (shape === "headers") {
      const [name, rest] = splitField(raw);
      keys.push({ from: at, to: at + name.length });
      const valueFrom = rest === undefined ? raw.length : raw.length - rest.trimStart().length;
      values.push({ from: at + valueFrom, to: at + raw.length });
    } else {
      const eq = raw.indexOf("=");
      keys.push({ from: at, to: at + (eq < 0 ? raw.length : eq) });
      values.push({ from: eq < 0 ? at + raw.length : at + eq + 1, to: at + raw.length });
    }
    at += raw.length + frame.join.length;
  }
  return { keys, values, jsonEscaped: false };
}

const LONE_SURROGATE = /[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/g;

const encodePart = (text: string) => encodeURIComponent(text.replace(LONE_SURROGATE, "�"));

const oneLine = (text: string) => text.replace(/\r?\n|\r/g, " ");

const NOT_IN_A_FIELD_NAME = /[^!#$%&'*+.^_`|~0-9A-Za-z-]/g;

const fieldName = (text: string) => (text.startsWith(":") ? ":" : "") + text.replace(NOT_IN_A_FIELD_NAME, "");

function editField(raw: string, part: PairPart, edited: string): string {
  const [name, rest] = splitField(raw);
  if (part === "value") return `${name}: ${oneLine(edited)}`;
  return rest === undefined ? fieldName(edited) : `${fieldName(edited)}:${rest}`;
}

function editEncoded(raw: string, part: PairPart, edited: string): string {
  const [key, value] = splitRaw(raw);
  if (part === "value") return `${key}=${encodePart(edited)}`;
  return raw.includes("=") ? `${encodePart(edited)}=${value}` : encodePart(edited);
}

export function editPair(text: string, index: number, part: PairPart, edited: string, shape: FormShape = "query"): string {
  const frame = frameOf(text, shape);
  const edit = shape === "headers" ? editField : editEncoded;
  return rebuild(
    frame,
    frame.raws.map((raw, i) => (i === index ? edit(raw, part, edited) : raw)),
  );
}

export function addPairAfter(text: string, index: number, shape: FormShape = "query"): string {
  const frame = frameOf(text, shape);
  const next = [...frame.raws];
  next.splice(index + 1, 0, shape === "headers" ? ":" : "=");
  return rebuild(frame, next);
}

export function removePair(text: string, index: number, shape: FormShape = "query"): string {
  const frame = frameOf(text, shape);
  return rebuild(
    frame,
    frame.raws.filter((_, i) => i !== index),
  );
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

export function sortByKey(text: string, shape: FormShape = "query"): string {
  const sorted = parsePairs(text, shape).sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
  return rebuild(
    frameOf(text, shape),
    sorted.map((pair) => pair.raw),
  );
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

function markPart(text: string, scans: readonly Scan[], limit: number) {
  return mergeRanges(
    scans.map((scan) => scan(text, limit)),
    limit,
  );
}

function scansFor(part: PairPart, chips: readonly FindChip[]): Scan[] {
  return chips.flatMap((chip) => {
    const scan = PART_KEYS[part].includes(chip.key) ? scanOf(chip) : null;
    return scan ? [scan] : [];
  });
}

export function markPairs(pairs: FormPair[], chips: readonly FindChip[], limit = MATCH_LIMIT): PairsFound {
  const byPair = new Map<number, PairMatches>();
  let total = 0;
  if (chips.length === 0) return { byPair, total, capped: false };
  const keyScans = scansFor("key", chips);
  const valueScans = scansFor("value", chips);
  for (const pair of pairs) {
    const key = markPart(pair.key, keyScans, limit - total);
    const value = markPart(pair.value, valueScans, limit - total - key.ranges.length);
    const found = key.ranges.length + value.ranges.length;
    if (found) byPair.set(pair.index, { first: total, key: key.ranges, value: value.ranges });
    total += found;
    if (key.capped || value.capped) return { byPair, total, capped: key.capped || value.capped };
  }
  return { byPair, total, capped: false };
}
