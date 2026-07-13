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

export interface FlowRow {
  time: string;
  method: string;
  host: string;
  path: string;
  status: string;
  tone: StatusTone;
  duration: string;
}

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
  return {
    time: new Date(flow.started_at).toLocaleTimeString(),
    method: flow.method,
    host,
    path,
    status: flow.status == null ? "…" : String(flow.status),
    tone: statusTone(flow.status),
    duration: flow.duration_ms == null ? "" : `${flow.duration_ms} ms`,
  };
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

// Reproduce the request as a runnable curl: method (omitted for GET), URL, each
// request header, and the captured request body. Pure — the panel copies its output.
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
