import type { BodyView, FlowView, HeaderView, SendFields } from "./api";
import { bodyNotice, humanBytes, rawKind, TEXT_MAX } from "./body";

// The request in the composer, before and after it is sent. `origin` is the
// row's index in the seed's header list, or null for a row the draft added —
// the one fact every mark is a comparison against.
export interface DraftRow {
  key: number;
  name: string;
  value: string;
  origin: number | null;
}

export interface Draft {
  seed: FlowView | null;
  method: string;
  url: string;
  rows: DraftRow[];
  body: string;
}

export type SeedBody =
  | { kind: "text"; text: string }
  | { kind: "empty" }
  | { kind: "binary"; size: number }
  | { kind: "absent"; sentence: string };

export type RowMark = "edited" | "added" | "follows" | null;

export type DraftRequest = { seedId: string; fields: SendFields } | { seedId: null; fields: SendFields };

const FROM_THE_CLIENT = "the client's to set";

// The core rewrites these whatever a draft says (DEC-58), so a row for one
// would show a value that is not sent.
const CORE_OWNED: Record<string, string> = {
  "content-length": "set from the body",
  connection: FROM_THE_CLIENT,
  "keep-alive": FROM_THE_CLIENT,
  "proxy-connection": FROM_THE_CLIENT,
  "transfer-encoding": FROM_THE_CLIENT,
  upgrade: FROM_THE_CLIENT,
  te: FROM_THE_CLIENT,
};

export function coreOwned(name: string): string | undefined {
  return CORE_OWNED[name.toLowerCase()];
}

export const CORE_OWNED_NAMES = Object.keys(CORE_OWNED);

export function droppedRows(seed: FlowView): HeaderView[] {
  return seed.request_headers.filter((h) => coreOwned(h.name) !== undefined);
}

// Every render of the composer asks, and the answer is fixed for the seed's
// life: a body up to TEXT_MAX is decoded once, not once per keystroke.
const decoded = new WeakMap<BodyView, SeedBody>();

export function seedBody(body: BodyView): SeedBody {
  let known = decoded.get(body);
  if (!known) {
    known = decode(body);
    decoded.set(body, known);
  }
  return known;
}

function decode(body: BodyView): SeedBody {
  switch (body.state) {
    case "empty":
      return { kind: "empty" };
    case "captured": {
      const bytes = Uint8Array.from(atob(body.data_base64), (c) => c.charCodeAt(0));
      if (bytes.length === 0) return { kind: "empty" };
      if (bytes.length > TEXT_MAX || rawKind(bytes) !== "text") return { kind: "binary", size: bytes.length };
      return { kind: "text", text: new TextDecoder().decode(bytes) };
    }
    default:
      return { kind: "absent", sentence: bodyNotice(body) ?? "" };
  }
}

export function binarySentence(size: number): string {
  return `binary body, ${humanBytes(size)} — sent as captured`;
}

export function draftFrom(seed: FlowView): Draft {
  const body = seedBody(seed.request_body);
  return {
    seed,
    method: seed.method,
    url: seed.uri,
    rows: seed.request_headers
      .map((h, origin) => ({ key: origin, name: h.name, value: h.value, origin }))
      .filter((row) => coreOwned(row.name) === undefined),
    body: body.kind === "text" ? body.text : "",
  };
}

export function blankDraft(): Draft {
  return { seed: null, method: "GET", url: "", rows: [], body: "" };
}

export function restart(draft: Draft): Draft {
  return draft.seed ? draftFrom(draft.seed) : blankDraft();
}

function seedRow(draft: Draft, row: DraftRow): HeaderView | null {
  return draft.seed && row.origin !== null ? draft.seed.request_headers[row.origin] : null;
}

export function rowMark(draft: Draft, row: DraftRow): RowMark {
  if (!draft.seed) return null;
  const from = seedRow(draft, row);
  if (!from) return "added";
  if (from.name !== row.name || from.value !== row.value) return "edited";
  return row.name.toLowerCase() === "host" ? "follows" : null;
}

// What the Host row shows while nobody has typed in it: the host the core will
// send, which is the URL's, not the seed's.
export function shownValue(draft: Draft, row: DraftRow): string {
  if (rowMark(draft, row) !== "follows") return row.value;
  try {
    return new URL(draft.url).host;
  } catch {
    return "";
  }
}

// The seed's Host row cannot go: the core sends the URL's host whenever the
// draft names none, so a removal would show a header as gone that is sent.
export function removable(draft: Draft, row: DraftRow): boolean {
  return seedRow(draft, row)?.name.toLowerCase() !== "host";
}

export function removedCount(draft: Draft): number {
  if (!draft.seed) return 0;
  const kept = new Set(draft.rows.map((row) => row.origin));
  return draft.seed.request_headers.filter((h, i) => coreOwned(h.name) === undefined && !kept.has(i)).length;
}

function headersChanged(draft: Draft): boolean {
  return removedCount(draft) > 0 || draft.rows.some((row) => {
    const mark = rowMark(draft, row);
    return mark === "edited" || mark === "added";
  });
}

// Whether the body goes at all. An unedited body is sent by omission, so the
// seed's bytes travel exactly as captured — including the ones never put in the
// field, which is why a blank field over a binary or absent seed body is not an
// edit.
export function bodyReplaced(draft: Draft): boolean {
  if (!draft.seed) return draft.body !== "";
  const body = seedBody(draft.seed.request_body);
  return body.kind === "text" ? draft.body !== body.text : draft.body !== "";
}

export function bodyRevertible(draft: Draft): boolean {
  return draft.seed !== null && seedBody(draft.seed.request_body).kind === "text" && bodyReplaced(draft);
}

export function revertBody(draft: Draft): Draft {
  const body = draft.seed ? seedBody(draft.seed.request_body) : null;
  return { ...draft, body: body?.kind === "text" ? body.text : "" };
}

export function isEdited(draft: Draft): boolean {
  if (!draft.seed) return draft.method !== "GET" || draft.url !== "" || draft.rows.length > 0 || draft.body !== "";
  return (
    draft.method !== draft.seed.method || draft.url !== draft.seed.uri || headersChanged(draft) || bodyReplaced(draft)
  );
}

export function canSend(draft: Draft): boolean {
  return draft.url.trim() !== "";
}

export function updateRow(draft: Draft, key: number, patch: Partial<Pick<DraftRow, "name" | "value">>): Draft {
  return { ...draft, rows: draft.rows.map((row) => (row.key === key ? { ...row, ...patch } : row)) };
}

export function revertRow(draft: Draft, key: number): Draft {
  const row = draft.rows.find((r) => r.key === key);
  const from = row && seedRow(draft, row);
  return from ? updateRow(draft, key, { name: from.name, value: from.value }) : draft;
}

export function removeRow(draft: Draft, key: number): Draft {
  return { ...draft, rows: draft.rows.filter((row) => row.key !== key) };
}

// Keys only have to be unique within one draft, and the seed's rows already
// hold 0..n-1, so the next one is past every key in use.
export function nextKey(draft: Draft): number {
  return Math.max(draft.seed ? draft.seed.request_headers.length - 1 : -1, ...draft.rows.map((r) => r.key)) + 1;
}

export function addRow(draft: Draft): Draft {
  return { ...draft, rows: [...draft.rows, { key: nextKey(draft), name: "", value: "", origin: null }] };
}

function wireRows(draft: Draft): HeaderView[] {
  return draft.rows
    .filter((row) => rowMark(draft, row) !== "follows")
    .map((row) => ({ name: row.name, value: row.value }));
}

// Only what differs from the seed is sent. A changed header list goes whole,
// minus an unedited Host row: a Host in a given list is sent as typed, and the
// seed's would carry the old host to a new URL.
export function toRequest(draft: Draft): DraftRequest {
  if (!draft.seed) {
    const fields: SendFields = { method: draft.method, url: draft.url };
    if (draft.rows.length > 0) fields.headers = wireRows(draft);
    if (draft.body !== "") fields.body = draft.body;
    return { seedId: null, fields };
  }
  const fields: SendFields = {};
  if (draft.method !== draft.seed.method) fields.method = draft.method;
  if (draft.url !== draft.seed.uri) fields.url = draft.url;
  if (headersChanged(draft)) fields.headers = wireRows(draft);
  if (bodyReplaced(draft)) fields.body = draft.body;
  return { seedId: draft.seed.id, fields };
}
