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

export interface StreamHandlers {
  onEvent: () => void;
  onLive?: (live: boolean) => void;
}

export interface TrawlerApi {
  listFlows(filter: Filter): Promise<FlowSummary[]>;
  getFlow(id: string): Promise<FlowView>;
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
    async getFlow(id) {
      const res = await fetch(`${base()}/flows/${encodeURIComponent(id)}`);
      if (!res.ok) throw new Error(`GET /flows/${id} failed: ${res.status}`);
      return (await res.json()) as FlowView;
    },
    stream({ onEvent, onLive }) {
      const source = new EventSource(`${base()}/stream`);
      for (const name of ["request", "response", "evicted", "lagged"]) {
        source.addEventListener(name, onEvent);
      }
      source.onopen = () => onLive?.(true);
      source.onerror = () => onLive?.(false);
      return () => source.close();
    },
  };
}
