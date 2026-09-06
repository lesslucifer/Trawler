// Typed client for the control API (:7421 by default, REST + SSE). The base URL
// is configurable — never a hardcoded port in component code. Dev/preview/tests
// inject it; the released app exposes it as configuration.

export interface FlowSummary {
  id: string;
  started_at: string;
  duration_ms: number | null;
  method: string;
  uri: string;
  status: number | null;
  request_body_state: string;
  response_body_state: string | null;
}

export interface HeaderView {
  name: string;
  value: string;
}

export type BodyView =
  | { state: "captured"; size: number; data_base64: string }
  | { state: "streaming"; content_type: string }
  | { state: "not_captured"; reason: string; actual: number | null }
  | { state: "empty" };

// Full flow projection from GET /flows/{id} — mirrors the server's FlowView.
export interface FlowView {
  id: string;
  started_at: string;
  duration_ms: number | null;
  method: string;
  uri: string;
  status: number | null;
  request_http_version: string;
  response_http_version: string | null;
  request_headers: HeaderView[];
  response_headers: HeaderView[];
  request_body: BodyView;
  response_body: BodyView | null;
}

export type Leg = "request" | "response";

export interface BodyFetch {
  bytes: Uint8Array;
  contentType: string | null;
}

export interface Filter {
  q?: string;
  host?: string;
  method?: string;
  status?: string;
}

const FILTER_KEYS: (keyof Filter)[] = ["q", "host", "method", "status"];

export function filterParams(filter: Filter): URLSearchParams {
  const params = new URLSearchParams();
  for (const key of FILTER_KEYS) {
    const value = filter[key]?.trim();
    if (value) params.set(key, value);
  }
  return params;
}

declare global {
  interface Window {
    __TRAWLER_API__?: string;
    __TRAWLER_PROXY__?: string;
  }
}

// Base URL resolution, most specific first: a runtime global, then a value that
// survives a reload via sessionStorage (both are the released app's configuration
// hook and how the e2e injects an ephemeral port without a rebuild), then the
// build-time env, then the dev default.
export function defaultBaseUrl(): string {
  if (typeof window !== "undefined") {
    if (window.__TRAWLER_API__) return window.__TRAWLER_API__;
    const stored = window.sessionStorage?.getItem("__TRAWLER_API__");
    if (stored) return stored;
  }
  return import.meta.env.VITE_TRAWLER_API ?? "http://127.0.0.1:7421";
}

// The proxy address resolves the same way as the base URL (DEC-12: configurable,
// never hardcoded in component code). Both snippets and the native set-proxy
// command consume it as data.
export function defaultProxyAddr(): string {
  if (typeof window !== "undefined") {
    if (window.__TRAWLER_PROXY__) return window.__TRAWLER_PROXY__;
    const stored = window.sessionStorage?.getItem("__TRAWLER_PROXY__");
    if (stored) return stored;
  }
  return import.meta.env.VITE_TRAWLER_PROXY ?? "127.0.0.1:7420";
}

export interface StreamHandlers {
  onEvent: () => void;
  // A tunnel reached the proxy (may never become a flow — untrusted-CA handshake).
  // Onboarding stamps its arrival and judges it by recency; the flow list ignores it.
  onConnection?: () => void;
}

export interface TrawlerApi {
  listFlows(filter: Filter): Promise<FlowSummary[]>;
  connectionCount(): Promise<number>;
  caCertPem(): Promise<string>;
  getFlow(id: string): Promise<FlowView>;
  getBody(id: string, leg: Leg, decode: boolean): Promise<BodyFetch>;
  stream(handlers: StreamHandlers): () => void;
}

export function createApi(baseUrl?: string): TrawlerApi {
  // Resolved per call, not captured, so a runtime __TRAWLER_API__ set after mount
  // (the e2e's injection) takes effect on the next fetch without a reload.
  const base = () => (baseUrl ?? defaultBaseUrl()).replace(/\/$/, "");
  return {
    async listFlows(filter) {
      const res = await fetch(`${base()}/flows?${filterParams(filter)}`);
      if (!res.ok) throw new Error(`GET /flows failed: ${res.status}`);
      return (await res.json()) as FlowSummary[];
    },
    async connectionCount() {
      const res = await fetch(`${base()}/stats`);
      if (!res.ok) throw new Error(`GET /stats failed: ${res.status}`);
      return ((await res.json()) as { connections: number }).connections;
    },
    async caCertPem() {
      const res = await fetch(`${base()}/ca.crt`);
      if (!res.ok) throw new Error(`GET /ca.crt failed: ${res.status}`);
      return res.text();
    },
    async getFlow(id) {
      const res = await fetch(`${base()}/flows/${encodeURIComponent(id)}`);
      if (!res.ok) throw new Error(`GET /flows/${id} failed: ${res.status}`);
      return (await res.json()) as FlowView;
    },
    async getBody(id, leg, decode) {
      // Raw asks for `source`, not the stored bytes: a compressed body's stored
      // form is a hex dump of gzip frames, which answers no question a reader
      // has. `source` reverses only the transport encoding, so raw stays the
      // bytes the server sent — unindented and untranscoded.
      const query = decode ? "?decode=1" : "?decode=source";
      const res = await fetch(`${base()}/flows/${encodeURIComponent(id)}/${leg}/body${query}`);
      if (!res.ok) throw new Error(`GET ${leg} body for ${id} failed: ${res.status}`);
      return {
        bytes: new Uint8Array(await res.arrayBuffer()),
        contentType: res.headers.get("content-type"),
      };
    },
    stream({ onEvent, onConnection }) {
      const source = new EventSource(`${base()}/stream`);
      // `connection` is deliberately absent here: a bare tunnel never becomes a
      // flow, so it must not trigger a /flows refetch — only bump the counter.
      for (const name of ["request", "response", "evicted", "lagged"]) {
        source.addEventListener(name, onEvent);
      }
      if (onConnection) source.addEventListener("connection", onConnection);
      return () => source.close();
    },
  };
}
