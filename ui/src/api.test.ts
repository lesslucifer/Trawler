import { afterEach, vi } from "vitest";
import { attachmentName, createApi, defaultProxyAddr, filterParams } from "./api";
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
      source: "captured",
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

test("exportHar carries the live filter and takes the filename the server stamped", async () => {
  const bytes = new TextEncoder().encode('{"log":{}}');
  const fetchMock = vi.fn().mockResolvedValue({
    ok: true,
    arrayBuffer: async () => bytes.buffer,
    headers: new Headers({ "content-disposition": 'attachment; filename="trawler-20260908-101500.har"' }),
  });
  vi.stubGlobal("fetch", fetchMock);

  const out = await createApi("http://x").exportHar({ host: "api.example.com", q: "" });
  expect(fetchMock).toHaveBeenCalledWith("http://x/flows/export?host=api.example.com");
  expect(out.filename).toBe("trawler-20260908-101500.har");
  expect([...out.bytes]).toEqual([...bytes]);
});

test("attachmentName reads quoted and bare forms, and falls back when the header is unreadable", () => {
  expect(attachmentName('attachment; filename="a b.har"')).toBe("a b.har");
  expect(attachmentName("attachment; filename=plain.har")).toBe("plain.har");
  // The header needs the CORS expose grant to be visible at all, so an older
  // core reads as null here rather than as a missing filename.
  expect(attachmentName(null)).toBe("trawler.har");
  expect(attachmentName("attachment")).toBe("trawler.har");
});

test("exportHar throws on a non-ok response", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 500 }));
  await expect(createApi("http://x").exportHar({})).rejects.toThrow(/500/);
});

test("importHar posts the bytes to the HAR door and answers the count", async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ imported: 3 }) });
  vi.stubGlobal("fetch", fetchMock);

  const bytes = new TextEncoder().encode('{"log":{"entries":[]}}');
  expect(await createApi("http://x").importHar(bytes)).toBe(3);
  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe("http://x/flows/import");
  expect(init.method).toBe("POST");
  expect(init.body).toBe(bytes);
});

test("importCurl posts the command verbatim under ?format=curl", async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ imported: 1 }) });
  vi.stubGlobal("fetch", fetchMock);

  const command = "curl 'https://api.example.com/v1/orders' \\\n  -H 'accept: application/json'";
  expect(await createApi("http://x").importCurl(command)).toBe(1);
  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe("http://x/flows/import?format=curl");
  expect(init.method).toBe("POST");
  // Verbatim: the command is the body, not a JSON-wrapped field, because a
  // wrapper adds a quoting layer to an input whose whole difficulty is quoting.
  expect(init.body).toBe(command);
});

test("a refused import throws the server's own sentence, unwrapped and unprefixed", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      ok: false,
      status: 400,
      text: async () => "unsupported flag --data-urlencode\n",
    }),
  );
  await expect(createApi("http://x").importCurl("curl -G http://x/")).rejects.toThrow(
    /^unsupported flag --data-urlencode$/,
  );
});

test("a refusal with no body still says something, rather than showing an empty notice", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 500, text: async () => "  " }));
  await expect(createApi("http://x").importHar(new Uint8Array())).rejects.toThrow(/500 and gave no reason/);
});

test("clearFlows DELETEs the listing under the live filter and answers what left", async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ removed: 214 }) });
  vi.stubGlobal("fetch", fetchMock);

  expect(await createApi("http://x").clearFlows({ method: "GET" })).toBe(214);
  expect(fetchMock).toHaveBeenCalledWith("http://x/flows?method=GET", { method: "DELETE" });
});

test("a refused clear throws the server's sentence too", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 400, text: async () => "status must be a number" }));
  await expect(createApi("http://x").clearFlows({ status: "x" })).rejects.toThrow(/status must be a number/);
});

test("stream listens for cleared — the server emits it and nothing was listening", () => {
  const listeners: Record<string, EventListener[]> = {};
  class FakeSource {
    addEventListener(name: string, fn: EventListener) {
      (listeners[name] ??= []).push(fn);
    }
    close() {}
  }
  vi.stubGlobal("EventSource", FakeSource);

  const onEvent = vi.fn();
  createApi("http://x").stream({ onEvent });
  listeners["cleared"].forEach((fn) => fn(new Event("cleared")));
  expect(onEvent).toHaveBeenCalledOnce();
});

test("listFlows parses the provenance mark off a summary", async () => {
  const rows: FlowSummary[] = [
    {
      id: "f1",
      started_at: "2026-07-12T10:00:00Z",
      duration_ms: null,
      method: "GET",
      uri: "http://x/",
      status: null,
      source: "imported",
      request_body_state: "captured",
      response_body_state: null,
    },
  ];
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, json: async () => rows }));
  expect((await createApi("http://x").listFlows({}))[0].source).toBe("imported");
});
