import { rawKind } from "./body";
import { signatureOf } from "./dataKind";
import { reformatJson } from "./jsonTools";
import { looksLikeTime, time, TIME_SHAPE, type TimeReading } from "./timeDecoder";

export type DecoderId = "jwt" | "base64" | "url" | "time";

export const DECODER_LABEL: Record<DecoderId, string> = { jwt: "JWT", base64: "base64", url: "URL", time: "time" };

export type Reading =
  | { kind: "json"; shown: string; text: string; bytes: Uint8Array }
  | { kind: "text"; text: string; bytes: Uint8Array }
  | { kind: "binary"; bytes: Uint8Array; looks: string | null }
  | { kind: "jwt"; header: string; payload: string; signature: string; text: string; bytes: Uint8Array }
  | TimeReading
  | { kind: "unreadable"; reason: string };

export type Decoded = Exclude<Reading, { kind: "unreadable" }>;

export type Peeked = { decoder: DecoderId; reading: Decoded } | { decoder: null; reason: string };

const DECODER_ORDER: DecoderId[] = ["jwt", "base64", "url", "time"];
const BASE64_DIGIT = /[A-Za-z0-9+/\-_]/;
const ESCAPE = /%[0-9A-Fa-f]{2}/;

const unreadable = (reason: string): Reading => ({ kind: "unreadable", reason });
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

function readBytes(bytes: Uint8Array): Reading {
  if (bytes.length === 0) return unreadable("it decodes to nothing");
  if (rawKind(bytes) === "hex") return { kind: "binary", bytes, looks: signatureOf(bytes)?.name ?? null };
  const text = new TextDecoder("utf-8", { ignoreBOM: true }).decode(bytes);
  if (/^\s*[[{]/.test(text)) {
    const laidOut = reformatJson(text, "  ", false, "\n");
    if (laidOut.ok) return { kind: "json", shown: laidOut.text, text, bytes };
  }
  return { kind: "text", text, bytes };
}

function sextet(char: string): number {
  if (char === "+" || char === "-") return 62;
  if (char === "/" || char === "_") return 63;
  const code = char.charCodeAt(0);
  if (code >= 97) return code - 71;
  if (code >= 65) return code - 65;
  return code + 4;
}

export function base64(input: string): Reading {
  const chars = [...input];
  const foreign = chars.findIndex((char) => !BASE64_DIGIT.test(char) && char !== "=" && !/\s/.test(char));
  if (foreign >= 0) return unreadable(`character ${foreign + 1}, “${chars[foreign]}”, is not in the base64 alphabet`);
  const compact = input.replace(/\s+/g, "");
  const digits = compact.replace(/=+$/, "");
  const padding = digits.indexOf("=");
  if (padding >= 0) return unreadable(`“=” at character ${padding + 1} is padding, which only ends base64`);
  if (digits.length % 4 === 1) {
    return unreadable(`${plural(digits.length, "character")} cannot be base64: a length of 4n + 1 never decodes`);
  }
  const bytes = new Uint8Array(Math.floor((digits.length * 6) / 8));
  let bits = 0;
  let held = 0;
  let at = 0;
  for (const char of digits) {
    held = (held << 6) | sextet(char);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      bytes[at++] = (held >> bits) & 0xff;
    }
  }
  return readBytes(bytes);
}

export function url(input: string): Reading {
  const bad = /%(?![0-9A-Fa-f]{2})/.exec(input);
  if (bad) {
    return unreadable(
      `“${input.slice(bad.index, bad.index + 3)}” at character ${bad.index + 1} is not a percent-escape: % takes two hex digits`,
    );
  }
  const encoder = new TextEncoder();
  const parts = input.split(/(%[0-9A-Fa-f]{2})/).map((part) =>
    ESCAPE.test(part) && part.length === 3
      ? Uint8Array.of(parseInt(part.slice(1), 16))
      : encoder.encode(part.replace(/\+/g, " ")),
  );
  const bytes = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  parts.reduce((offset, part) => {
    bytes.set(part, offset);
    return offset + part.length;
  }, 0);
  return readBytes(bytes);
}

function segment(part: string, name: string): Reading {
  const read = base64(part);
  if (read.kind === "unreadable") return unreadable(`the ${name} is not base64url: ${read.reason}`);
  if (read.kind !== "json") return unreadable(`the ${name} decodes, but not to JSON`);
  return read;
}

export function jwt(input: string): Reading {
  const parts = input.trim().split(".");
  if (parts.length !== 3) {
    return unreadable(`${plural(parts.length, "part")} separated by dots. A JWT has 3: header, payload, signature`);
  }
  const header = segment(parts[0], "header");
  if (header.kind !== "json") return header;
  const payload = segment(parts[1], "payload");
  if (payload.kind !== "json") return payload;
  return { kind: "jwt", header: header.shown, payload: payload.shown, signature: parts[2], text: payload.text, bytes: payload.bytes };
}

const DECODERS: Record<DecoderId, (input: string) => Reading> = { jwt, base64, url, time: (input) => time(input) };

export function guessDecoder(input: string): DecoderId {
  if (looksLikeTime(input)) return "time";
  if (/^eyJ[\w-]*\.[\w-]*/.test(input)) return "jwt";
  return input.includes("%") ? "url" : "base64";
}

function means(id: DecoderId, reading: Reading, input: string): reading is Decoded {
  if (reading.kind === "unreadable") return false;
  return id !== "url" || ESCAPE.test(input);
}

// Only a timestamp reads two ways: its digits are base64 too, and the guess
// puts time first. Otherwise base64 and a JWT refuse "%", a JWT's dots refuse
// base64, URL means something only over an escape, and the rest refuse ":".
export function decode(input: string): Peeked {
  const guess = guessDecoder(input);
  const guessed = DECODERS[guess](input);
  for (const id of [guess, ...DECODER_ORDER.filter((other) => other !== guess)]) {
    const reading = id === guess ? guessed : DECODERS[id](input);
    if (means(id, reading, input)) return { decoder: id, reading };
  }
  return { decoder: null, reason: guessed.kind === "unreadable" ? guessed.reason : "it decodes to nothing readable" };
}

export interface Span {
  from: number;
  to: number;
}

export interface Found {
  range: Span;
  peeked: Peeked;
}

const VALUE_CHAR = /[A-Za-z0-9+/=_\-.%]/;
const VALUE_RUN = /[A-Za-z0-9+/=_\-.%]+/g;
const MOST_RUNS_TRIED = 8;

const inValue = (text: string, at: number) => at >= 0 && at < text.length && VALUE_CHAR.test(text[at]);
const width = (span: Span) => span.to - span.from;

function trimmed(text: string, { from, to }: Span): Span {
  while (from < to && !inValue(text, from)) from++;
  while (to > from && !inValue(text, to - 1)) to--;
  return { from, to };
}

function grown(text: string, { from, to }: Span): Span {
  while (inValue(text, from - 1)) from--;
  while (inValue(text, to)) to++;
  return { from, to };
}

function candidates(text: string, range: Span): Span[] {
  const exact = trimmed(text, range);
  const whole = grown(text, exact);
  const runs = Array.from(text.slice(whole.from, whole.to).matchAll(VALUE_RUN), (match) => ({
    from: whole.from + match.index,
    to: whole.from + match.index + match[0].length,
  }))
    .sort((a, b) => width(b) - width(a))
    .slice(0, MOST_RUNS_TRIED);
  const seen = new Set<string>();
  return [exact, whole, ...runs].filter((span) => {
    const id = `${span.from}:${span.to}`;
    if (width(span) === 0 || seen.has(id)) return false;
    seen.add(id);
    return true;
  });
}

export const hasValueAround = (text: string, range: Span) => width(grown(text, trimmed(text, range))) > 0;

const readsWell = (peeked: Peeked) =>
  peeked.decoder !== null && !(peeked.reading.kind === "binary" && peeked.reading.looks === null);

function dateTimeAround(text: string, range: Span): Span | null {
  const lineFrom = text.lastIndexOf("\n", range.from - 1) + 1;
  const lineEnd = text.indexOf("\n", range.to);
  const lineTo = lineEnd < 0 ? text.length : lineEnd;
  for (const match of text.slice(lineFrom, lineTo).matchAll(TIME_SHAPE)) {
    const span = { from: lineFrom + match.index, to: lineFrom + match.index + match[0].length };
    const outside = text.slice(range.from, Math.max(range.from, span.from)) + text.slice(Math.min(range.to, span.to), range.to);
    if (span.from <= range.to && range.from <= span.to && !VALUE_CHAR.test(outside)) return span;
  }
  return null;
}

export function decodeAround(text: string, range: Span): Found | null {
  const dateTime = dateTimeAround(text, range);
  if (dateTime) return { range: dateTime, peeked: decode(text.slice(dateTime.from, dateTime.to)) };
  let first: Found | null = null;
  let decodes: Found | null = null;
  for (const span of candidates(text, range)) {
    const found = { range: span, peeked: decode(text.slice(span.from, span.to)) };
    if (readsWell(found.peeked)) return found;
    first ??= found;
    if (found.peeked.decoder !== null) decodes ??= found;
  }
  return decodes ?? first;
}

export function replacement(reading: Decoded): string | null {
  return reading.kind === "binary" ? null : reading.text;
}

export function opensAs(reading: Decoded): { bytes: Uint8Array; typeHint?: string } {
  if (reading.kind === "binary") return { bytes: reading.bytes };
  return { bytes: reading.bytes, typeHint: reading.kind === "text" || reading.kind === "time" ? "text/plain" : "application/json" };
}

export async function contentKey(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return `decoded-${Array.from(digest.subarray(0, 16), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}
