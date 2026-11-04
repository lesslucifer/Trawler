import { bodyKind, rawKind, type BodyKind } from "./body";

export type DataKind = BodyKind;

export const DATA_KINDS: { id: DataKind; label: string }[] = [
  { id: "json", label: "JSON" },
  { id: "form", label: "form" },
  { id: "xml", label: "XML / HTML" },
  { id: "text", label: "text" },
  { id: "hex", label: "hex" },
  { id: "image", label: "image" },
];

export const READ_ONLY_KINDS: ReadonlySet<DataKind> = new Set(["hex", "image"]);

export interface Detected {
  kind: DataKind;
  from: string;
}

export const FROM_BYTES = "its bytes";

export interface Signature {
  name: string;
  image: boolean;
}

const SIGNATURES: (Signature & { bytes: number[] })[] = [
  { name: "PNG", image: true, bytes: [0x89, 0x50, 0x4e, 0x47] },
  { name: "JPEG", image: true, bytes: [0xff, 0xd8, 0xff] },
  { name: "GIF", image: true, bytes: [0x47, 0x49, 0x46, 0x38] },
  { name: "gzip", image: false, bytes: [0x1f, 0x8b] },
  { name: "PDF", image: false, bytes: [0x25, 0x50, 0x44, 0x46] },
];

export function signatureOf(bytes: Uint8Array): Signature | null {
  const found = SIGNATURES.find((signature) => signature.bytes.every((byte, i) => bytes[i] === byte));
  return found ? { name: found.name, image: found.image } : null;
}

function sniff(bytes: Uint8Array): DataKind {
  if (signatureOf(bytes)?.image) return "image";
  if (rawKind(bytes) === "hex") return "hex";
  const text = new TextDecoder().decode(bytes).trimStart();
  if (text.startsWith("{") || text.startsWith("[")) {
    try {
      JSON.parse(text);
      return "json";
    } catch {
      return "text";
    }
  }
  return text.startsWith("<") ? "xml" : "text";
}

export function detectKind(typeHint: string | undefined, bytes: Uint8Array): Detected {
  const mime = (typeHint ?? "").split(";")[0].trim().toLowerCase();
  const hinted = mime ? bodyKind(mime) : null;
  if (hinted !== null && hinted !== "hex") return { kind: hinted, from: mime };
  return { kind: sniff(bytes), from: FROM_BYTES };
}

export const kindLabel = (kind: DataKind) => DATA_KINDS.find((k) => k.id === kind)?.label ?? kind;
