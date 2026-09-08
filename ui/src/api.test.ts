import { afterEach, vi } from "vitest";
import { ApiError, apiAddr, createApi, defaultProxyAddr, filterParams } from "./api";
import type { FlowSummary } from "./api";

afterEach(() => {
  vi.restoreAllMocks();
  delete window.__TRAWLER_PROXY__;
  window.sessionStorage.clear();
});

test("defaultProxyAddr resolves global, then session, then the default", () => {
  expect(defaultProxyAddr()).toBe("127.0.0.1:7420");

  window.sessionStorage.setItem("__TRAWLER_PROXY__", "10.0.0.2:8080");
  expect(defaultProxyAddr()).toBe("10.0.0.2:8080");

  window.__TRAWLER_PROXY__ = "192.168.1.9:7000";
  expect(defaultProxyAddr()).toBe("192.168.1.9:7000");
});

test("filterParams sets only non-empty, trimmed keys", () => {
  const p = filterParams({ q: " get ", host: "", method: "POST", status: "  " });
  expect(p.get("q")).toBe("get");
  expect(p.get("method")).toBe("POST");
  expect(p.has("host")).toBe(false);
  expect(p.has("status")).toBe(false);
});

test("listFlows fetches /flows with the filter and parses typed rows", async () => {
  const rows: FlowSummary[] = [
    {
      id: "f1",
      started_at: "2026-07-12T10:00:00Z",
      duration_ms: 5,
      method: "GET",
      uri: "http://x/",
      status: 200,
      request_body_state: "empty",
      response_body_state: "empty",
    },
  ];
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => rows });
  vi.stubGlobal("fetch", fetchMock);

  const api = createApi("http://127.0.0.1:9999/");
  const result = await api.listFlows({ host: "x" });

  expect(fetchMock).toHaveBeenCalledWith("http://127.0.0.1:9999/flows?host=x");
  expect(result[0].method).toBe("GET");
  expect(result[0].status).toBe(200);
});

test("caCertPem fetches /ca.crt as text and throws on a non-ok response", async () => {
  const pem = "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----\n";
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, text: async () => pem });
  vi.stubGlobal("fetch", fetchMock);
  await expect(createApi("http://127.0.0.1:9999").caCertPem()).resolves.toBe(pem);
  expect(fetchMock).toHaveBeenCalledWith("http://127.0.0.1:9999/ca.crt");

  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 404 }));
  await expect(createApi("http://x").caCertPem()).rejects.toThrow(/404/);
});

test("listFlows throws on a non-ok response", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 500 }));
  await expect(createApi("http://x").listFlows({})).rejects.toThrow(/500/);
});

test("connectionCount reads /stats and returns the connections tally", async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ connections: 3 }) });
  vi.stubGlobal("fetch", fetchMock);
  await expect(createApi("http://127.0.0.1:9999").connectionCount()).resolves.toBe(3);
  expect(fetchMock).toHaveBeenCalledWith("http://127.0.0.1:9999/stats");
});

// A bare CONNECT tunnel never becomes a flow, so a `connection` event must bump
// the counter (onConnection) and must NOT trigger the flow-list refetch (onEvent).
test("stream routes a connection event to onConnection only, never onEvent", () => {
  const listeners: Record<string, EventListener[]> = {};
  class FakeSource {
    onopen: (() => void) | null = null;
    onerror: (() => void) | null = null;
    addEventListener(name: string, fn: EventListener) {
      (listeners[name] ??= []).push(fn);
    }
    close() {}
  }
  vi.stubGlobal("EventSource", FakeSource);

  const onEvent = vi.fn();
  const onConnection = vi.fn();
  createApi("http://x").stream({ onEvent, onConnection });

  listeners["connection"].forEach((fn) => fn(new Event("connection")));
  expect(onConnection).toHaveBeenCalledOnce();
  expect(onEvent).not.toHaveBeenCalled();

  listeners["request"].forEach((fn) => fn(new Event("request")));
  expect(onEvent).toHaveBeenCalledOnce();
});

test("getBody asks for the rendered view or the source, and keeps the content type", async () => {
  const bytes = new TextEncoder().encode("hi");
  const fetchMock = vi.fn().mockResolvedValue({
    ok: true,
    arrayBuffer: async () => bytes.buffer,
    headers: new Headers({ "content-type": "text/plain" }),
  });
  vi.stubGlobal("fetch", fetchMock);
  const api = createApi("http://x");

  const decoded = await api.getBody("f1", "response", true);
  expect(fetchMock).toHaveBeenLastCalledWith("http://x/flows/f1/response/body?decode=1");
  expect(decoded.contentType).toBe("text/plain");
  expect([...decoded.bytes]).toEqual([...bytes]);

  // Raw asks for source, not the stored bytes: a gzipped body's stored form is
  // a hex dump of compression frames, which answers nothing.
  await api.getBody("f1", "request", false);
  expect(fetchMock).toHaveBeenLastCalledWith("http://x/flows/f1/request/body?decode=source");
});

test("getBody throws on a non-ok response", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 404 }));
  await expect(createApi("http://x").getBody("f1", "response", true)).rejects.toThrow(/404/);
});

// The whole basis of the shell's two connection sentences: a core that refused
// and a core that said nothing must not arrive at the caller as the same throw.
test("a refused listing throws ApiError with the status; a dead socket throws no ApiError", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 400 }));
  const refused = await createApi("http://x")
    .listFlows({ status: "4xz" })
    .catch((e: unknown) => e);
  expect(refused).toBeInstanceOf(ApiError);
  expect((refused as ApiError).status).toBe(400);

  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
  const silent = await createApi("http://x")
    .listFlows({})
    .catch((e: unknown) => e);
  expect(silent).not.toBeInstanceOf(ApiError);
});

test("every non-ok answer carries its own status, not a flattened one", async () => {
  for (const status of [400, 404, 500, 503]) {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status }));
    const error = await createApi("http://x")
      .getFlow("f1")
      .catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).status).toBe(status);
  }
});

// The sentences name an address, and DEC-12 says that address is configuration.
test("apiAddr follows the configured base URL, without its scheme", () => {
  expect(apiAddr()).toBe("127.0.0.1:7421");

  window.__TRAWLER_API__ = "http://127.0.0.1:53219";
  expect(apiAddr()).toBe("127.0.0.1:53219");
  delete window.__TRAWLER_API__;
});

// The stream's own health is a separate channel from its events: it is what the
// shell reads to say a core died under an open window, with no user act at all.
test("stream reports its transport going down and coming back", () => {
  const listeners: Record<string, EventListener[]> = {};
  class FakeSource {
    addEventListener(name: string, fn: EventListener) {
      (listeners[name] ??= []).push(fn);
    }
    close() {}
  }
  vi.stubGlobal("EventSource", FakeSource);

  const onEvent = vi.fn();
  const onDown = vi.fn();
  const onUp = vi.fn();
  createApi("http://x").stream({ onEvent, onDown, onUp });

  listeners["error"].forEach((fn) => fn(new Event("error")));
  expect(onDown).toHaveBeenCalledOnce();
  listeners["open"].forEach((fn) => fn(new Event("open")));
  expect(onUp).toHaveBeenCalledOnce();
  // Transport health is not a flow event — it must not itself be read as traffic.
  expect(onEvent).not.toHaveBeenCalled();
});
