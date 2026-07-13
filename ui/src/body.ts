import type { BodyView, Leg } from "./api";

// Content-type → renderer dispatch, the core UC-5 contract. Decoded bodies
// dispatch on the served content type; raw bodies sniff bytes (rawKind),
// because stored bytes may be compressed regardless of what the type claims.
export type BodyKind = "json" | "image" | "form" | "text" | "hex";

export function bodyKind(contentType: string | null): BodyKind {
  const mime = (contentType ?? "").split(";")[0].trim().toLowerCase();
  if (mime === "application/json" || mime.endsWith("+json")) return "json";
  if (mime.startsWith("image/")) return "image";
  if (mime === "application/x-www-form-urlencoded") return "form";
  if (
    mime.startsWith("text/") ||
    mime === "application/xml" ||
    mime.endsWith("+xml") ||
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

// ponytail: hex view caps at 4 KiB — a full 2 MiB capture would be ~128k DOM
// lines; raise the cap or virtualize if inspecting deep binary ever matters.
export const HEX_MAX = 4096;

export function hexDump(bytes: Uint8Array, max: number = HEX_MAX): string {
  const shown = bytes.subarray(0, max);
  const lines: string[] = [];
  for (let off = 0; off < shown.length; off += 16) {
    const chunk = Array.from(shown.subarray(off, off + 16));
    const hex = chunk.map((b) => b.toString(16).padStart(2, "0")).join(" ");
    const ascii = chunk.map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".")).join("");
    lines.push(`${off.toString(16).padStart(8, "0")}  ${hex.padEnd(47)}  |${ascii}|`);
  }
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
