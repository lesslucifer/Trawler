import { rawKind } from "./body";
import { kindLabel, signatureOf } from "./dataKind";
import type { FormShape } from "./formPairs";

export type ContentKind = "json" | "form" | "xml" | "html" | "javascript" | "css" | "text" | "hex" | "image";

export interface KindReading {
  kind: ContentKind;
  by: "label" | "bytes";
  label: string | null;
}

export const HEADERS_LABEL = "text/rfc822-headers";
export const URI_LABEL = "text/uri-list";

export const FORM_LABEL_SHAPES: ReadonlyMap<string, FormShape> = new Map([
  ["application/x-www-form-urlencoded", "query"],
  [HEADERS_LABEL, "headers"],
  [URI_LABEL, "url"],
]);

const JAVASCRIPT = new Set([
  "application/javascript",
  "text/javascript",
  "application/x-javascript",
  "application/ecmascript",
  "text/ecmascript",
]);

export function normalizeLabel(label: string | null | undefined): string | null {
  const mime = (label ?? "").split(";")[0].trim().toLowerCase();
  return mime || null;
}

export function labelKind(label: string | null | undefined): ContentKind | null {
  const mime = normalizeLabel(label);
  if (mime === null) return null;
  if (mime === "application/json" || mime === "text/json" || mime.endsWith("+json")) return "json";
  if (FORM_LABEL_SHAPES.has(mime)) return "form";
  if (mime === "text/html" || mime === "application/xhtml+xml") return "html";
  if (mime === "application/xml" || mime === "text/xml" || mime.endsWith("+xml")) return "xml";
  if (JAVASCRIPT.has(mime)) return "javascript";
  if (mime === "text/css") return "css";
  if (mime.startsWith("image/")) return "image";
  return null;
}

const LEADING_NOISE = /^(?:\s|<!--[\s\S]*?-->|<\?xml[\s\S]*?\?>)*/;
const HTML_START = /(?:<!doctype\s+html\b|<(?:html|head|body)(?=[\s>/]))/iy;
const MARKUP_START = /^\s*</;
const DOCUMENT_START = /^\s*[[{]/;

export function hasHtmlSignature(text: string): boolean {
  HTML_START.lastIndex = LEADING_NOISE.exec(text)![0].length;
  return HTML_START.test(text);
}

export function isWellFormedXml(text: string): boolean {
  if (!MARKUP_START.test(text) || typeof DOMParser === "undefined") return false;
  const parsed = new DOMParser().parseFromString(text, "application/xml");
  return parsed.getElementsByTagName("parsererror").length === 0;
}

function isJsonDocument(text: string): boolean {
  if (!DOCUMENT_START.test(text)) return false;
  try {
    const value: unknown = JSON.parse(text);
    return typeof value === "object" && value !== null;
  } catch {
    return false;
  }
}

function markupKind(text: string, labelled: ContentKind | null): ContentKind {
  switch (labelled) {
    case "html":
      return "html";
    case "xml":
      if (isWellFormedXml(text)) return "xml";
      return hasHtmlSignature(text) ? "html" : "xml";
    case "json":
      if (hasHtmlSignature(text)) return "html";
      return isWellFormedXml(text) ? "xml" : "json";
    case null:
      if (hasHtmlSignature(text)) return "html";
      return isWellFormedXml(text) ? "xml" : "text";
    default:
      return labelled;
  }
}

function decideKind(bytes: Uint8Array, labelled: ContentKind | null): ContentKind {
  if (labelled === "image" || signatureOf(bytes)?.image) return "image";
  if (rawKind(bytes) === "hex") return "hex";
  const text = new TextDecoder().decode(bytes);
  if (isJsonDocument(text)) return "json";
  return markupKind(text, labelled);
}

export function readKind(bytes: Uint8Array, label?: string | null): KindReading {
  const served = normalizeLabel(label);
  const labelled = labelKind(served);
  const kind = decideKind(bytes, labelled);
  return { kind, by: kind === labelled ? "label" : "bytes", label: served };
}

export function overruled({ kind, by, label }: KindReading): boolean {
  if (by !== "bytes" || label === null) return false;
  return labelKind(label) !== null || (kind !== "text" && kind !== "hex");
}

export function servedAs(reading: KindReading): string | null {
  return overruled(reading) ? `${kindLabel(reading.kind)} · served as ${reading.label}` : null;
}

export function decidedBy(reading: KindReading, picked: boolean): string {
  if (picked) return `read ${kindLabel(reading.kind)} from ${reading.by === "bytes" ? "its bytes" : reading.label}`;
  if (overruled(reading)) return `read from its bytes · served as ${reading.label}`;
  return reading.label === null ? "read from its bytes" : `from ${reading.label}`;
}
