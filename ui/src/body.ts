import type { BodyView, Leg } from "./api";

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
export function humanBytes(bytes: number): string {
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit > 0 && value < 10 ? 1 : 0;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

// Caps count characters (a rendered line is characters), but a cut marker states
// bytes — the size the user would save. For non-ASCII the two differ.
export function byteLength(text: string): number {
  return new TextEncoder().encode(text).length;
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

// null = the body is captured and viewable; a string is the notice to show
// instead of a viewer — states never render blank.
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
      return body.reason === "too_large"
        ? `not captured — too large (${body.actual} bytes)`
        : "not captured — read error";
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
  [(m) => m.startsWith("text/") || m === "application/x-www-form-urlencoded", ".txt"],
];

export function saveFilename(flowId: string, leg: Leg, contentType: string | null): string {
  const mime = (contentType ?? "").split(";")[0].trim().toLowerCase();
  const ext = EXTENSIONS.find(([test]) => test(mime))?.[1] ?? ".bin";
  return `${flowId.slice(0, 8)}-${leg}${ext}`;
}

// Native app → OS save dialog (dialog plugin registers the picked path in the
// fs plugin's scope); plain browser (vite dev) → anchor download.
export async function saveBody(bytes: Uint8Array, filename: string): Promise<void> {
  if ("__TAURI_INTERNALS__" in window) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { writeFile } = await import("@tauri-apps/plugin-fs");
    const path = await save({ defaultPath: filename });
    if (path) await writeFile(path, bytes);
    return;
  }
  const url = URL.createObjectURL(new Blob([bytes]));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
