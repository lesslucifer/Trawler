// Typed client for the control API (:7421 by default, REST + SSE). The base URL
// is configurable — never a hardcoded port in component code. Dev/preview/tests
// inject it; the released app exposes it as configuration.

export type FlowSource = "captured" | "imported" | "sent";

// The encodings the core serves its CA in, one per kind of installer.
export type CaFormat = "pem" | "cer" | "mobileconfig";

export interface FlowSummary {
  id: string;
  started_at: string;
  duration_ms: number | null;
  method: string;
  uri: string;
  status: number | null;
  source: FlowSource;
  seed: string | null;
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
  source: FlowSource;
  seed: string | null;
  request_http_version: string;
  response_http_version: string | null;
  request_headers: HeaderView[];
  response_headers: HeaderView[];
  request_body: BodyView;
  response_body: BodyView | null;
}

export type Leg = "request" | "response";

// What the two send doors take. On a replay an absent field is the seed's own
// value; from scratch, method and url are required and the core says so.
export interface SendFields {
  method?: string;
  url?: string;
  headers?: HeaderView[];
  body?: string;
}

export interface BodyFetch {
  bytes: Uint8Array;
  contentType: string | null;
}

export interface HarExport {
  bytes: Uint8Array;
  filename: string;
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

// The API answered and refused. Its absence is the other outcome: a fetch that
// rejects never produces one, so `instanceof ApiError` is how a caller tells a
// core that said no from a core that said nothing — the distinction the shell
// renders as two different sentences.
export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
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

// The address the shell's connection sentences name. Derived, never written
// down: DEC-12 keeps the API location configurable, so the words on screen have
// to follow it.
export function apiAddr(): string {
  return defaultBaseUrl().replace(/^https?:\/\//, "");
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

// One address a second device could route to. The core enumerates them so the
// screen offers a choice that exists rather than a field to type into.
export interface ReachInterface {
  name: string;
  address: string;
}

// What GET /reach answers. The three addresses are null unless reach is armed —
// the screen must be able to tell "not reachable" from "reachable here", and a
// plausible-looking address for a socket nobody is listening on is the failure
// this whole seam exists to prevent.
export interface ReachState {
  armed: boolean;
  proxy: string | null;
  api: string | null;
  ca_urls: Record<CaFormat, string> | null;
  wifi_page: string | null;
  interfaces: ReachInterface[];
}

// Nothing armed, nothing promised: the state a fresh core reports and the state
// a screen assumes until one answers.
export const UNREACHABLE: ReachState = {
  armed: false,
  proxy: null,
  api: null,
  ca_urls: null,
  wifi_page: null,
  interfaces: [],
};

export interface StreamHandlers {
  onEvent: () => void;
  // A tunnel reached the proxy (may never become a flow — untrusted-CA handshake).
  // Onboarding stamps its arrival and judges it by recency; the flow list ignores it.
  // The authority is the tunnel's host:port, the one thing a CONNECT names.
  onConnection?: (authority: string) => void;
  // The stream itself went down or came back. This is the only signal that
  // arrives without the user doing anything, so it is what lets the shell state
  // a core that died under an open window rather than waiting for the next
  // keystroke to discover it.
  onDown?: () => void;
  onUp?: () => void;
}

export interface TrawlerApi {
  listFlows(filter: Filter): Promise<FlowSummary[]>;
  // The newest row the proxy captured, whatever the list is filtered to: the
  // header's dot reads it (DEC-61), and a filter must not quieten it.
  latestCapture(): Promise<FlowSummary | null>;
  exportHar(filter: Filter): Promise<HarExport>;
  importHar(bytes: Uint8Array): Promise<number>;
  importCurl(command: string): Promise<number>;
  clearFlows(filter: Filter): Promise<number>;
  caCertPem(): Promise<string>;
  caFile(format: CaFormat): Promise<Uint8Array>;
  getFlow(id: string): Promise<FlowView>;
  getBody(id: string, leg: Leg, pretty: boolean): Promise<BodyFetch>;
  reach(): Promise<ReachState>;
  // Exposing this machine to a network is an authorization act, not a setting
  // (DEC-55): the shell's confirm comes first, and nothing here is persisted.
  armReach(address: string): Promise<ReachState>;
  disarmReach(): Promise<ReachState>;
  replay(id: string, fields: SendFields): Promise<FlowView>;
  send(fields: SendFields): Promise<FlowView>;
  stream(handlers: StreamHandlers): () => void;
}

export function createApi(baseUrl?: string): TrawlerApi {
  // Resolved per call, not captured, so a runtime __TRAWLER_API__ set after mount
  // (the e2e's injection) takes effect on the next fetch without a reload.
  const base = () => (baseUrl ?? defaultBaseUrl()).replace(/\/$/, "");
  return {
    async listFlows(filter) {
      const res = await fetch(`${base()}/flows?${filterParams(filter)}`);
      if (!res.ok) throw new ApiError(res.status, `GET /flows failed: ${res.status}`);
      return (await res.json()) as FlowSummary[];
    },
    async latestCapture() {
      const res = await fetch(`${base()}/flows?source=captured&limit=1`);
      if (!res.ok) throw new ApiError(res.status, `GET /flows failed: ${res.status}`);
      return ((await res.json()) as { flows: FlowSummary[] }).flows[0] ?? null;
    },
    async exportHar(filter) {
      const res = await fetch(`${base()}/flows/export?${filterParams(filter)}`);
      // A door, not a reader: its failure is shown to a person beside the
      // control they pressed, so it carries the server's sentence like the other
      // two do. The readers above stay status-shaped — nothing renders those.
      if (!res.ok) throw new Error(await refusal(res));
      return {
        bytes: new Uint8Array(await res.arrayBuffer()),
        filename: attachmentName(res.headers.get("content-disposition")),
      };
    },
    async importHar(bytes) {
      return imported(
        await fetch(`${base()}/flows/import`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: bytes,
        }),
      );
    },
    async importCurl(command) {
      return imported(
        await fetch(`${base()}/flows/import?format=curl`, {
          method: "POST",
          headers: { "content-type": "text/plain" },
          body: command,
        }),
      );
    },
    async clearFlows(filter) {
      const res = await fetch(`${base()}/flows?${filterParams(filter)}`, { method: "DELETE" });
      if (!res.ok) throw new Error(await refusal(res));
      return ((await res.json()) as { removed: number }).removed;
    },
    async caCertPem() {
      const res = await fetch(`${base()}/ca.pem`);
      if (!res.ok) throw new ApiError(res.status, `GET /ca.pem failed: ${res.status}`);
      return res.text();
    },
    async caFile(format) {
      const res = await fetch(`${base()}/ca.${format}`);
      if (!res.ok) throw new ApiError(res.status, `GET /ca.${format} failed: ${res.status}`);
      return new Uint8Array(await res.arrayBuffer());
    },
    async getFlow(id) {
      const res = await fetch(`${base()}/flows/${encodeURIComponent(id)}`);
      if (!res.ok) throw new ApiError(res.status, `GET /flows/${id} failed: ${res.status}`);
      return (await res.json()) as FlowView;
    },
    async getBody(id, leg, pretty) {
      // Both views are decoded — the parameter picks which resolution. Raw asks
      // for `source`, not the stored bytes: a compressed body's stored form is a
      // hex dump of gzip frames, which answers no question a reader has.
      // `source` reverses only the transport encoding, so raw stays the bytes
      // the server sent — unindented and untranscoded.
      const query = pretty ? "?decode=1" : "?decode=source";
      const res = await fetch(`${base()}/flows/${encodeURIComponent(id)}/${leg}/body${query}`);
      if (!res.ok) throw new ApiError(res.status, `GET ${leg} body for ${id} failed: ${res.status}`);
      return {
        bytes: new Uint8Array(await res.arrayBuffer()),
        contentType: res.headers.get("content-type"),
      };
    },
    async reach() {
      const res = await fetch(`${base()}/reach`);
      if (!res.ok) throw new ApiError(res.status, `GET /reach failed: ${res.status}`);
      return (await res.json()) as ReachState;
    },
    async armReach(address) {
      const res = await fetch(`${base()}/reach`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ address }),
      });
      // A refusal here is read by the person who just confirmed, so it carries
      // the server's own sentence — which address, and why it could not listen.
      if (!res.ok) throw new Error(await refusal(res));
      return (await res.json()) as ReachState;
    },
    async disarmReach() {
      const res = await fetch(`${base()}/reach`, { method: "DELETE" });
      if (!res.ok) throw new Error(await refusal(res));
      return (await res.json()) as ReachState;
    },
    async replay(id, fields) {
      return sent(await fetch(`${base()}/flows/${encodeURIComponent(id)}/replay`, jsonPost(fields)));
    },
    async send(fields) {
      return sent(await fetch(`${base()}/flows`, jsonPost(fields)));
    },
    stream({ onEvent, onConnection, onDown, onUp }) {
      const source = new EventSource(`${base()}/stream`);
      // `connection` is deliberately absent here: a bare tunnel never becomes a
      // flow, so it must not trigger a /flows refetch — only bump the counter.
      for (const name of ["request", "response", "evicted", "lagged", "cleared"]) {
        source.addEventListener(name, onEvent);
      }
      if (onConnection)
        source.addEventListener("connection", (event) => onConnection(authorityOf(event)));
      // EventSource reconnects on its own, so `error` is "the stream is not up
      // right now" rather than "give up"; `open` is the recovery it pairs with.
      if (onDown) source.addEventListener("error", onDown);
      if (onUp) source.addEventListener("open", onUp);
      return () => source.close();
    },
  };
}

function authorityOf(event: Event): string {
  try {
    const { authority } = JSON.parse((event as MessageEvent<string>).data) as { authority?: unknown };
    return typeof authority === "string" ? authority : "";
  } catch {
    return "";
  }
}

// The server writes its refusals as one lowercase sentence naming what it did
// not understand, for a reader rather than for a log — so the client throws that
// sentence unchanged and never wraps it. A refusal with no body would leave the
// screen with nothing to show, which is the only case that gets a sentence here.
async function refusal(res: Response): Promise<string> {
  const reason = (await res.text()).trim();
  return reason || `the server refused with ${res.status} and gave no reason`;
}

function jsonPost(fields: SendFields): RequestInit {
  return { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(fields) };
}

// A send that got no reply is a refusal with the core's sentence (502 · 504),
// exactly like one the core would not attempt (400 · 404 · 422): the screen
// shows either verbatim beside the button that sent it.
async function sent(res: Response): Promise<FlowView> {
  if (!res.ok) throw new Error(await refusal(res));
  return (await res.json()) as FlowView;
}

async function imported(res: Response): Promise<number> {
  if (!res.ok) throw new Error(await refusal(res));
  return ((await res.json()) as { imported: number }).imported;
}

const EXPORT_FALLBACK_FILENAME = "trawler.har";

// The export's name is the server's to choose — it stamps each one so successive
// exports do not overwrite each other. Reading it needs the CORS grant to expose
// the header, so the fallback is a real path (an older core, a proxy that strips
// it), not a defensive nicety.
export function attachmentName(contentDisposition: string | null): string {
  const quoted = /filename="([^"]+)"/.exec(contentDisposition ?? "");
  if (quoted) return quoted[1];
  const bare = /filename=([^;]+)/.exec(contentDisposition ?? "");
  return bare ? bare[1].trim() : EXPORT_FALLBACK_FILENAME;
}
