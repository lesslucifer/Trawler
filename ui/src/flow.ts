import type { FlowSummary } from "./api";

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
