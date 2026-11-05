import type { ContentKind } from "./contentKind";

export type DataKind = ContentKind;

export const DATA_KINDS: { id: DataKind; label: string }[] = [
  { id: "json", label: "JSON" },
  { id: "form", label: "form" },
  { id: "xml", label: "XML" },
  { id: "html", label: "HTML" },
  { id: "javascript", label: "JavaScript" },
  { id: "css", label: "CSS" },
  { id: "text", label: "text" },
  { id: "hex", label: "hex" },
  { id: "image", label: "image" },
];

export const READ_ONLY_KINDS: ReadonlySet<DataKind> = new Set(["hex", "image"]);

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

export const kindLabel = (kind: DataKind) => DATA_KINDS.find((k) => k.id === kind)?.label ?? kind;
