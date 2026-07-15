import { afterEach, vi } from "vitest";
import { createApi, defaultProxyAddr, filterParams } from "./api";
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
