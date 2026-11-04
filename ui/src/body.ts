import type { BodyFetch, BodyView, FlowView, Leg } from "./api";
import type { DataPayload } from "./dataWindow";

// Content-type → renderer dispatch, the core UC-5 contract. Decoded bodies
// dispatch on the served content type; raw bodies sniff bytes (rawKind),
// because stored bytes may be compressed regardless of what the type claims.
export type BodyKind = "json" | "xml" | "image" | "form" | "text" | "hex";

export function bodyKind(contentType: string | null): BodyKind {
  const mime = (contentType ?? "").split(";")[0].trim().toLowerCase();
  if (mime === "application/json" || mime.endsWith("+json")) return "json";
  if (mime.startsWith("image/")) return "image";
  if (mime === "application/x-www-form-urlencoded") return "form";
  if (
    mime === "application/xml" ||
    mime === "text/xml" ||
    mime === "text/html" ||
    mime.endsWith("+xml")
  ) {
    return "xml";
  }
  if (
    mime.startsWith("text/") ||
    mime === "application/javascript" ||
    mime === "application/ecmascript"
  ) {
    return "text";
  }
  return "hex";
}

export function rawKind(bytes: Uint8Array): "text" | "hex" {
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    return /[\x00-\x08\x0b\x0c\x0e-\x1f]/.test(text) ? "hex" : "text";
  } catch {
    return "hex";
  }
}

export function bodyText(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes);
}

export interface DecodedBody {
  text: string;
  whole: boolean;
}

// Whether every byte decoded is a fact only the decoder holds. Bytes that are
// not valid UTF-8 yield one replacement character each, and each of those
// re-encodes to three — so past that point a size taken from the bytes and a
// size taken from the text have stopped measuring the same thing, and a reader
// downstream cannot tell a replacement character from one the body carried on
// purpose. The strict pass is the same single decode the lenient one was; only
// a body that fails it is decoded twice.
export function decodeBody(bytes: Uint8Array): DecodedBody {
  try {
    return { text: new TextDecoder("utf-8", { fatal: true }).decode(bytes), whole: true };
  } catch {
    return { text: bodyText(bytes), whole: false };
  }
}

// ponytail: hex view caps at 4 KiB — a full capture-cap body would be ~600k DOM
// lines; raise the cap or virtualize if inspecting deep binary ever matters.
export const HEX_MAX = 4096;

// ponytail: JSON over 128 KiB renders as text, not a tree — an unbounded tree
// on a 10 MiB body means tens of thousands of synchronous DOM nodes; virtualize
// with @tanstack/react-virtual (already a dep) if tree-view of huge JSON matters.
export const TREE_MAX = 131_072;

// The text/xml/form render cap the BodyRenderer family left open at design time.
// Shares TREE_MAX's order of magnitude: one cap for every character-rendered
// member, so a body's cut point never depends on which member drew it.
export const TEXT_MAX = TREE_MAX;

// Cut markers state sizes the way a human reads them — raw byte counts mean
// nothing at a glance (body-renderer.html DEC-12).
export function humanBytes(bytes: number, extraDigits = 0): string {
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : (value < 10 ? 1 : 0) + extraDigits;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

// A cut marker must never read "showing 128 KiB of 128 KiB": rounding can make a
// real truncation look like none, which is the exact false statement the
// never-truncate-silently rule exists to prevent. Borrow precision until the two
// sizes differ, and fall back to exact bytes if even that collides.
export function humanBytePair(shown: number, total: number): [string, string] {
  for (let extra = 0; extra <= 3; extra += 1) {
    const left = humanBytes(shown, extra);
    const right = humanBytes(total, extra);
    if (left !== right) return [left, right];
  }
  return [`${shown} B`, `${total} B`];
}

// Caps count characters (a rendered line is characters), but a cut marker states
// bytes — the size the user would save. For non-ASCII the two differ.
export function byteLength(text: string): number {
  let length = 0;
  for (let i = 0; i < text.length; i += 1) {
    const code = text.charCodeAt(i);
    const next = text.charCodeAt(i + 1);
    if (code < 0x80) length += 1;
    else if (code < 0x800) length += 2;
    else if (code >= 0xd800 && code < 0xdc00 && next >= 0xdc00 && next < 0xe000) {
      length += 4;
      i += 1;
    } else length += 3;
  }
  return length;
}

// One dump, two consumers: HexRenderer tones the three parts separately, while
// copy-to-clipboard needs the flat text. Both read the same rows, so the pane
// and the clipboard cannot disagree.
export interface HexRow {
  offset: string;
  hex: string;
  ascii: string;
}

export function hexRows(bytes: Uint8Array, max: number = HEX_MAX): HexRow[] {
  const shown = bytes.subarray(0, max);
  const rows: HexRow[] = [];
  for (let off = 0; off < shown.length; off += 16) {
    const chunk = Array.from(shown.subarray(off, off + 16));
    rows.push({
      offset: off.toString(16).padStart(8, "0"),
      hex: chunk.map((b) => b.toString(16).padStart(2, "0")).join(" ").padEnd(47),
      ascii: `|${chunk.map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".")).join("")}|`,
    });
  }
  return rows;
}

export function hexDump(bytes: Uint8Array, max: number = HEX_MAX): string {
  const lines = hexRows(bytes, max).map((r) => `${r.offset}  ${r.hex}  ${r.ascii}`);
  if (bytes.length > max) lines.push(`… ${bytes.length - max} more bytes`);
  return lines.join("\n");
}

export function formEntries(text: string): [string, string][] {
  return [...new URLSearchParams(text)];
}

// A form body is cut on a pair boundary, never at a character offset: an offset
// can land mid-%XX (decodes to a stray literal) or mid-escape-run (a truncated
// multi-byte character decodes to U+FFFD), and that half-decoded value is what
// the row hands to the clipboard. A first pair that alone exceeds the cap keeps
// its key with an empty value — the field stays named, and no partial value can
// be copied as if it were whole.
export function formPairCut(text: string, at: number): number {
  const boundary = text.lastIndexOf("&", at);
  if (boundary > 0) return boundary;
  const key = text.indexOf("=");
  return key > 0 && key < at ? key + 1 : 0;
}

// null = the body is captured and viewable; a string is the notice to show
// instead of a viewer — states never render blank.
//
// Sizes are human-readable for the same reason CutMarker's are: the two can sit
// three lines apart in one pane, and a raw byte count beside "9.7 MiB" reads as
// a different kind of number rather than the same fact.
export function bodyNotice(body: BodyView | null): string | null {
  if (body == null) return "no response yet";
  switch (body.state) {
    case "captured":
      return null;
    case "empty":
      return "empty body";
    case "streaming":
      return `streaming (${body.content_type}) — passed through, not captured`;
    case "not_captured":
      if (body.reason !== "too_large") return "not captured — read error";
      return body.actual == null
        ? "not captured — too large"
        : `not captured — too large (${humanBytes(body.actual)})`;
  }
}

const EXTENSIONS: [test: (mime: string) => boolean, ext: string][] = [
  [(m) => m === "image/png", ".png"],
  [(m) => m === "image/jpeg", ".jpg"],
  [(m) => m === "image/gif", ".gif"],
  [(m) => m === "image/webp", ".webp"],
  [(m) => m === "image/avif", ".avif"],
  [(m) => m === "image/svg+xml", ".svg"],
  [(m) => m === "application/json" || m.endsWith("+json"), ".json"],
  [(m) => m === "text/html", ".html"],
  [(m) => m === "application/xml" || m === "text/xml" || m.endsWith("+xml"), ".xml"],
  [(m) => m === "application/javascript" || m === "text/javascript", ".js"],
  [(m) => m === "text/css", ".css"],
  [(m) => m === "text/csv", ".csv"],
  [(m) => m === "application/x-www-form-urlencoded", ".txt"],
  [(m) => m.startsWith("text/"), ".txt"],
];

export function bodyExtension(contentType: string | null): string {
  const mime = (contentType ?? "").split(";")[0].trim().toLowerCase();
  return EXTENSIONS.find(([test]) => test(mime))?.[1] ?? ".bin";
}

// A saved body is found again in a Downloads folder, where a flow id says
// nothing: the endpoint is what the reader remembers. Take the last path
// segment, fall back to the host when the path names nothing (a bare "/"), and
// keep the leg so a request and its response never collide.
function endpointSlug(uri: string): string {
  let host = "";
  let path = uri.split("?")[0];
  try {
    const parsed = new URL(uri);
    host = parsed.hostname;
    // A percent-escape the path cannot decode (truncated, or %zz) throws rather
    // than returning its input — the still-encoded form is a usable name.
    try {
      path = decodeURIComponent(parsed.pathname);
    } catch {
      path = parsed.pathname;
    }
  } catch {
    // Not a URL at all; the whole string is the best name available.
  }

  const segment = path.split("/").filter(Boolean).pop() ?? "";
  // A path segment that already carries an extension (…/app.js) would stack a
  // second one, and the served content type is the authority anyway. A host is
  // never trimmed — the dot in example.com is not an extension.
  const trimmed = segment.replace(/\.[A-Za-z0-9]{1,8}$/, "");
  // A name that is only separators, or leads with one, reads as broken and is
  // hostile to a shell — so a wholly non-ASCII segment (an IDN path) sanitizes
  // to nothing and the host, which is punycode and always safe, is the better
  // answer than a generic "body".
  return clean(trimmed) || clean(segment) || clean(host) || "body";
}

function clean(part: string): string {
  return part.replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^[-._]+|[-._]+$/g, "");
}

// A data window knows only its opener's caption, so that caption is the name a
// saved file is found again by; the hint, when there is one, picks the extension.
export function captionFilename(caption: string, typeHint: string | undefined, suffix = ""): string {
  const name = clean(caption).slice(0, 80).replace(/[-._]+$/, "") || "data";
  return `${name}${suffix}${bodyExtension(typeHint ?? null)}`;
}

export function saveFilename(uri: string, leg: Leg, contentType: string | null): string {
  return `${endpointSlug(uri)}-${leg}${bodyExtension(contentType)}`;
}

// Native app → OS save dialog (dialog plugin registers the picked path in the
// fs plugin's scope); plain browser (vite dev) → anchor download.
// Returns the name the bytes landed under, or null when the native dialog was
// cancelled. Both are load-bearing for the setup screen, which states an outcome:
// a cancel resolves like a write, and `filename` is only the dialog's pre-filled
// default, so neither "did it save" nor "under what name" is knowable without it.
// A dialog that opens in a folder the steps can name saves a file the steps
// find without the user choosing anything (W7's C1); a folder that cannot be
// read leaves the dialog where it opens by default.
async function startingPath(filename: string, folder?: () => Promise<string>): Promise<string> {
  if (folder === undefined) return filename;
  const { join } = await import("@tauri-apps/api/path");
  return folder().then(
    (dir) => join(dir, filename),
    () => filename,
  );
}

// What the native save returns, as opposed to the bare name a browser's
// download resolves to: a path a command can name.
export const isFullPath = (path: string) => /^(\/|[A-Za-z]:[\\/]|\\\\)/.test(path);

export async function saveBody(
  bytes: Uint8Array,
  filename: string,
  folder?: () => Promise<string>,
): Promise<string | null> {
  if ("__TAURI_INTERNALS__" in window) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { writeFile } = await import("@tauri-apps/plugin-fs");
    const path = await save({ defaultPath: await startingPath(filename, folder) });
    if (!path) return null;
    await writeFile(path, bytes);
    return path;
  }
  const url = URL.createObjectURL(new Blob([bytes]));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
  return filename;
}

export interface ChosenFile {
  name: string;
  bytes: Uint8Array;
}

// The inverse of saveBody, and it carries the same contract: null means the user
// dismissed the picker, which is an outcome and not a failure. The browser half
// exists for vite dev and the unit tests, where there is no OS dialog to open.
export async function openFile(extensions: string[]): Promise<ChosenFile | null> {
  if ("__TAURI_INTERNALS__" in window) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { readFile } = await import("@tauri-apps/plugin-fs");
    const path = await open({ multiple: false, directory: false, filters: [{ name: extensions.join("/"), extensions }] });
    if (typeof path !== "string") return null;
    return { name: path, bytes: await readFile(path) };
  }
  return pickThroughInput(extensions);
}

function pickThroughInput(extensions: string[]): Promise<ChosenFile | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = extensions.map((e) => `.${e}`).join(",");
    // A dismissed picker fires no change event in every browser, so `cancel` is
    // the only signal that the user chose nothing — without it the caller waits
    // on a promise that never settles.
    input.addEventListener("cancel", () => resolve(null));
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) return resolve(null);
      // A read that fails must reject rather than never settle: the caller
      // disables its controls for the whole wait, so a hung promise is a dialog
      // with no way forward and no way back.
      file
        .arrayBuffer()
        .then((buffer) => resolve({ name: file.name, bytes: new Uint8Array(buffer) }), reject);
    });
    input.click();
  });
}

// What the pane hands a data window: the bytes it holds, in the mode it shows.
// Raw bytes go without a hint, because a served type says nothing about stored
// bytes that may still be compressed; the window sniffs them, as rawKind does.
export function bodyPayload(flow: FlowView, leg: Leg, fetched: BodyFetch, pretty: boolean): DataPayload {
  const mode = pretty ? "pretty" : "raw";
  const where = flow.uri.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
  const payload: DataPayload = {
    key: `${flow.id}:${leg}:${mode}`,
    title: `${leg} body${pretty ? "" : " · raw"} · ${flow.method} ${where}`,
    bytes: fetched.bytes,
  };
  if (pretty && fetched.contentType) payload.typeHint = fetched.contentType;
  return payload;
}
