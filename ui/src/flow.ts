import type { BodyView, FlowSummary, FlowView } from "./api";
import type { Traffic } from "./verdicts";

export type StatusTone = "ok" | "warn" | "muted";

// 2xx → ok, 4xx/5xx → warn, pending/other → muted. The functional status color
// a debugging proxy needs — the one exception to Warp's no-chromatic-accent rule.
export function statusTone(status: number | null): StatusTone {
  if (status == null) return "muted";
  const klass = Math.floor(status / 100);
  if (klass === 2) return "ok";
  if (klass === 4 || klass === 5) return "warn";
  return "muted";
}

// Only a row the proxy captured says anything about a device's setup: an
// imported or sent row carries an https:// URI no device routed.
export function isCaptured(flow: FlowSummary): boolean {
  return flow.source === "captured";
}

// A decrypted HTTPS flow is the sole proof onboarding worked (E-UISETUP.W4): the
// MITM rewrites a decrypted request to absolute form, so an https:// URI means
// the exchange was intercepted and decrypted; a plain proxied request stays
// http:// and does not count. `since` is the earliest start that counts.
export function isDecryptedHttps(flows: FlowSummary[], since?: number): boolean {
  return flows.some(
    (f) =>
      isCaptured(f) &&
      f.uri.startsWith("https://") &&
      (since === undefined || Date.parse(f.started_at) >= since),
  );
}

// When the newest flow of each kind started, as epoch ms. The setup screen reads
// recency off these — a flow that is old proves nothing about now — so the time
// is the flow's own started_at, never the moment the list arrived.
export interface FlowTimes {
  lastDecryptedAt: number | null;
  lastPlainAt: number | null;
}

export function latestFlowTimes(flows: FlowSummary[], since?: number): FlowTimes {
  const times: FlowTimes = { lastDecryptedAt: null, lastPlainAt: null };
  for (const flow of flows) {
    if (!isCaptured(flow)) continue;
    const at = Date.parse(flow.started_at);
    if (Number.isNaN(at) || (since !== undefined && at < since)) continue;
    const key = flow.uri.startsWith("https://") ? "lastDecryptedAt" : "lastPlainAt";
    times[key] = Math.max(times[key] ?? at, at);
  }
  return times;
}

// The newest captured request of either scheme: http proves the proxy is in
// the path as surely as https does, and the dot asks nothing more (DEC-61).
export function lastCaptureAt(flows: FlowSummary[]): number | null {
  const { lastDecryptedAt, lastPlainAt } = latestFlowTimes(flows);
  if (lastDecryptedAt === null) return lastPlainAt;
  return lastPlainAt === null ? lastDecryptedAt : Math.max(lastDecryptedAt, lastPlainAt);
}

// The setup screen reads one setup session: what was captured, and the newest
// tunnel, from the moment that session opened. Traffic from before it is some
// earlier session's evidence (DEC-60). A null `since` bounds nothing.
export function sessionTraffic(
  flows: FlowSummary[],
  since: number | null,
  lastTunnelAt: number | null,
): Traffic {
  const from = since ?? undefined;
  return {
    ...latestFlowTimes(flows, from),
    lastTunnelAt: lastTunnelAt !== null && (from === undefined || lastTunnelAt >= from) ? lastTunnelAt : null,
  };
}

export interface FlowRow {
  host: string;
  path: string;
}

// The host alone, for callers that want no other part of the URI. Asked of every
// flow on every refresh, over a list that is mostly the same hosts as the last
// one — so the parse is remembered under the URI's authority, which is the only
// part a host is derived from. The memo never answers on its own: a miss parses
// the same string the full URI would have parsed to, so a stale or evicted entry
// costs time and never correctness.
const HOST_MEMO_MAX = 1024;

let hostByAuthority = new Map<string, string>();

export function hostOf(uri: string): string {
  const key = authority(uri);
  const remembered = hostByAuthority.get(key);
  if (remembered !== undefined) return remembered;

  let host = "";
  try {
    host = new URL(key).host;
  } catch {
    // relative/opaque URI — it names no host
  }

  if (hostByAuthority.size >= HOST_MEMO_MAX) hostByAuthority = new Map();
  hostByAuthority.set(key, host);
  return host;
}

// Everything up to the path, when the URI has that shape at all. A URI whose
// authority is ended by "?" or "#" rather than "/", and one with no "://" to
// measure from, key on themselves — correct, and simply not shared.
function authority(uri: string): string {
  const scheme = uri.indexOf("://");
  if (scheme < 0) return uri;
  const path = uri.indexOf("/", scheme + 3);
  return path < 0 ? uri : uri.slice(0, path);
}

// Splits a flow's URI into the two payload cells the list shows side by side.
// The row's other four values are passed raw to the components that own their
// formatting — TimestampText, MethodLabel, StatusPill and DurationText each
// refuse a pre-formatted value, so this is the only transform left here.
export function toRow(flow: FlowSummary): FlowRow {
  let host = "";
  let path = flow.uri;
  try {
    const u = new URL(flow.uri);
    host = u.host;
    path = u.pathname + u.search;
  } catch {
    // relative/opaque URI — show it whole as the path
  }
  return { host, path };
}

// Captured bytes → a UTF-8 string. atob yields Latin1-per-char, so re-encode the
// bytes before decoding as UTF-8; non-captured legs have no body text.
export function bodyText(body: BodyView): string {
  if (body.state !== "captured") return "";
  const bytes = Uint8Array.from(atob(body.data_base64), (c) => c.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

function shellQuote(s: string): string {
  return `'${s.replace(/'/g, "'\\''")}'`;
}

// -X is omitted for GET, matching curl's own default, so the common case reads clean.
export function toCurl(flow: FlowView): string {
  const parts = ["curl"];
  if (flow.method !== "GET") parts.push("-X", flow.method);
  parts.push(shellQuote(flow.uri));
  for (const h of flow.request_headers) {
    parts.push("-H", shellQuote(`${h.name}: ${h.value}`));
  }
  const body = bodyText(flow.request_body);
  if (body) parts.push("--data-raw", shellQuote(body));
  return parts.join(" ");
}
