import type { BodyView, FlowSummary, FlowView } from "./api";

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

// A decrypted HTTPS flow is the sole proof onboarding worked (E-UISETUP.W4): the
// MITM rewrites a decrypted request to absolute form, so an https:// URI means
// the exchange was intercepted and decrypted; a plain proxied request stays
// http:// and does not count.
export function isDecryptedHttps(flows: FlowSummary[]): boolean {
  return flows.some((f) => f.uri.startsWith("https://"));
}

export interface FlowRow {
  host: string;
  path: string;
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
