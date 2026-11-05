import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

// The data window's whole input (DEC-76): what the opener holds, by value. The
// window learns nothing else about where the data came from, and parses neither
// the title nor the heading - each is drawn as the opener wrote it.
export interface Heading {
  what: string;
  method: string;
  host: string;
  path: string;
  query: string;
}

export interface DataPayload {
  key: string;
  title: string;
  bytes: Uint8Array;
  typeHint?: string;
  heading?: Heading;
}

const HEADING_PARTS = ["what", "method", "host", "path", "query"] as const;

const isHeading = (value: unknown): value is Heading =>
  typeof value === "object" && value !== null && HEADING_PARTS.every((part) => typeof (value as Record<string, unknown>)[part] === "string");

export function queryCount(query: string): number {
  return query.replace(/^\?/, "").split("&").filter(Boolean).length;
}

type Caption = Omit<DataPayload, "bytes">;
type Invoke = <T>(command: string, args?: InvokeArgs) => Promise<T>;

export const DATA_WINDOW_PREFIX = "data-";
export const PAYLOAD_REGISTRY = "__trawlerDataPayloads";
const LENGTH_BYTES = 4;

export function dataWindowLabel(key: string): string {
  const hex = Array.from(new TextEncoder().encode(key), (byte) => byte.toString(16).padStart(2, "0"));
  return DATA_WINDOW_PREFIX + hex.join("");
}

export function framePayload({ bytes, ...caption }: DataPayload): Uint8Array {
  const { key, title, typeHint, heading } = caption;
  const json = new TextEncoder().encode(JSON.stringify({ key, title, typeHint, heading }));
  const framed = new Uint8Array(LENGTH_BYTES + json.length + bytes.length);
  new DataView(framed.buffer).setUint32(0, json.length);
  framed.set(json, LENGTH_BYTES);
  framed.set(bytes, LENGTH_BYTES + json.length);
  return framed;
}

export function unframePayload(framed: Uint8Array): DataPayload | null {
  if (framed.length < LENGTH_BYTES) return null;
  const length = new DataView(framed.buffer, framed.byteOffset, framed.byteLength).getUint32(0);
  if (framed.length < LENGTH_BYTES + length) return null;
  let caption: Caption;
  try {
    caption = JSON.parse(new TextDecoder().decode(framed.subarray(LENGTH_BYTES, LENGTH_BYTES + length))) as Caption;
  } catch {
    return null;
  }
  if (typeof caption?.key !== "string" || typeof caption.title !== "string") return null;
  const payload: DataPayload = { key: caption.key, title: caption.title, bytes: framed.slice(LENGTH_BYTES + length) };
  if (typeof caption.typeHint === "string") payload.typeHint = caption.typeHint;
  if (isHeading(caption.heading)) {
    const { what, method, host, path, query } = caption.heading;
    payload.heading = { what, method, host, path, query };
  }
  return payload;
}

type Registry = Map<string, DataPayload>;
type RegistryHost = Window & { [PAYLOAD_REGISTRY]?: Registry };

const registry = (host: RegistryHost): Registry => (host[PAYLOAD_REGISTRY] ??= new Map());
const browserWindows = new Map<string, Window>();

export function openInBrowser(payload: DataPayload, host: Window = window): void {
  const name = dataWindowLabel(payload.key);
  const open = browserWindows.get(name);
  if (open && !open.closed) {
    open.focus();
    return;
  }
  const child = host.open(host.location.pathname, name);
  if (!child) return;
  registry(host).set(name, payload);
  browserWindows.set(name, child);
}

export function takeInBrowser(self: Window = window): DataPayload | null {
  if (!self.name.startsWith(DATA_WINDOW_PREFIX)) return null;
  try {
    const parked = (self.opener as RegistryHost | null)?.[PAYLOAD_REGISTRY];
    const payload = parked?.get(self.name) ?? null;
    parked?.delete(self.name);
    return payload;
  } catch {
    return null;
  }
}

const inShell = () => "__TAURI_INTERNALS__" in window;

export async function openDataWindow(payload: DataPayload, call: Invoke = invoke): Promise<void> {
  if (!inShell()) return openInBrowser(payload);
  await call("open_data_window", framePayload(payload));
}

export async function takeDataPayload(call: Invoke = invoke): Promise<DataPayload | null> {
  if (!inShell()) return takeInBrowser();
  const framed = await call<ArrayBuffer>("take_data_payload");
  return framed.byteLength === 0 ? null : unframePayload(new Uint8Array(framed));
}

export function isDataWindow(self: Window = window): boolean {
  const label = "__TAURI_INTERNALS__" in self ? getCurrentWindow().label : self.name;
  return label.startsWith(DATA_WINDOW_PREFIX);
}

type Take = () => Promise<DataPayload | null>;
const claims = new WeakMap<Take, Promise<DataPayload | null>>();

// The payload is claimed exactly once (DEC-76 rule 2), and a second claim answers
// null — so a screen that mounts twice must share the first claim's answer.
export function claimOnce(take: Take = takeDataPayload): Promise<DataPayload | null> {
  let claim = claims.get(take);
  if (!claim) {
    claim = take();
    claims.set(take, claim);
  }
  return claim;
}
