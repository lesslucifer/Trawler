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
  onLive?: (live: boolean) => void;
  // A tunnel reached the proxy (may never become a flow — untrusted-CA handshake).
  // Onboarding's proxy step goes green on this alone; the flow list ignores it.
  onConnection?: () => void;
}

export interface TrawlerApi {
  listFlows(filter: Filter): Promise<FlowSummary[]>;
  connectionCount(): Promise<number>;
  caCertPem(): Promise<string>;
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
    stream({ onEvent, onLive, onConnection }) {
      const source = new EventSource(`${base()}/stream`);
      // `connection` is deliberately absent here: a bare tunnel never becomes a
      // flow, so it must not trigger a /flows refetch — only bump the counter.
      for (const name of ["request", "response", "evicted", "lagged"]) {
        source.addEventListener(name, onEvent);
      }
      if (onConnection) source.addEventListener("connection", onConnection);
      source.onopen = () => onLive?.(true);
      source.onerror = () => onLive?.(false);
      return () => source.close();
    },
  };
}
