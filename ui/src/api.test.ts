import { afterEach, vi } from "vitest";
import { ApiError, apiAddr, attachmentName, createApi, defaultProxyAddr, filterParams } from "./api";
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
      seed: null,
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
// One fake EventSource for every stream test: they all need the same thing —
// a handle on the listeners the client registered, so the test can fire one.
function stubEventSource() {
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
  return listeners;
}

// the counter (onConnection) and must NOT trigger the flow-list refetch (onEvent).
test("stream routes a connection event to onConnection only, never onEvent", () => {
  const listeners = stubEventSource();

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

  const pretty = await api.getBody("f1", "response", true);
  expect(fetchMock).toHaveBeenLastCalledWith("http://x/flows/f1/response/body?decode=1");
  expect(pretty.contentType).toBe("text/plain");
  expect([...pretty.bytes]).toEqual([...bytes]);

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

test("a refused export throws the server's sentence too — all three doors report the same way", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({ ok: false, status: 400, text: async () => "status must be a number" }),
  );
  await expect(createApi("http://x").exportHar({ status: "x" })).rejects.toThrow(
    /^status must be a number$/,
  );

  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 500, text: async () => "" }));
  await expect(createApi("http://x").exportHar({})).rejects.toThrow(/500 and gave no reason/);
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
  const listeners = stubEventSource();

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

test("stream listens for cleared — the server emits it and nothing was listening", () => {
  const listeners = stubEventSource();

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
      seed: null,
      request_body_state: "captured",
      response_body_state: null,
    },
  ];
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, json: async () => rows }));
  expect((await createApi("http://x").listFlows({}))[0].source).toBe("imported");
});

const UNREACHABLE = { armed: false, proxy: null, api: null, ca_cert_url: null, interfaces: [] };

test("reach reads the current state and the interfaces on offer", async () => {
  const state = {
    ...UNREACHABLE,
    interfaces: [{ name: "en0", address: "192.168.1.20" }],
  };
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => state });
  vi.stubGlobal("fetch", fetchMock);

  const result = await createApi("http://x").reach();

  expect(fetchMock).toHaveBeenCalledWith("http://x/reach");
  expect(result.armed).toBe(false);
  expect(result.proxy).toBeNull();
  expect(result.interfaces[0].address).toBe("192.168.1.20");
});

test("armReach posts the chosen address and returns what actually bound", async () => {
  const armed = {
    armed: true,
    proxy: "192.168.1.20:7420",
    api: "192.168.1.20:7421",
    ca_cert_url: "http://192.168.1.20:7421/ca.crt",
    interfaces: [{ name: "en0", address: "192.168.1.20" }],
  };
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => armed });
  vi.stubGlobal("fetch", fetchMock);

  const result = await createApi("http://x").armReach("192.168.1.20");

  expect(fetchMock).toHaveBeenCalledWith("http://x/reach", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ address: "192.168.1.20" }),
  });
  expect(result.ca_cert_url).toBe("http://192.168.1.20:7421/ca.crt");
});

test("a refused arm throws the server's own sentence, for the person who confirmed", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      ok: false,
      status: 400,
      text: async () => "could not listen on 192.0.2.1:7420: Can't assign requested address",
    }),
  );

  await expect(createApi("http://x").armReach("192.0.2.1")).rejects.toThrow(
    "could not listen on 192.0.2.1:7420: Can't assign requested address",
  );
});

test("disarmReach deletes and reports this machine only", async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => UNREACHABLE });
  vi.stubGlobal("fetch", fetchMock);

  const result = await createApi("http://x").disarmReach();

  expect(fetchMock).toHaveBeenCalledWith("http://x/reach", { method: "DELETE" });
  expect(result.armed).toBe(false);
  expect(result.api).toBeNull();
});

test("replay posts the overrides as JSON to the seed's door and answers the new flow", async () => {
  const flow = { id: "new-1", source: "sent", seed: "seed/1" };
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => flow });
  vi.stubGlobal("fetch", fetchMock);

  const fields = { headers: [{ name: "Authorization", value: "Bearer NEW" }] };
  const result = await createApi("http://x").replay("seed/1", fields);

  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe("http://x/flows/seed%2F1/replay");
  expect(init.method).toBe("POST");
  expect(init.headers).toEqual({ "content-type": "application/json" });
  expect(JSON.parse(init.body)).toEqual(fields);
  expect(result).toBe(flow);
});

test("an unchanged replay posts {} — the seed as it was", async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({}) });
  vi.stubGlobal("fetch", fetchMock);

  await createApi("http://x").replay("s", {});

  expect(fetchMock.mock.calls[0][1].body).toBe("{}");
});

test("send posts a request from scratch to /flows", async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ id: "n" }) });
  vi.stubGlobal("fetch", fetchMock);

  await createApi("http://x").send({ method: "GET", url: "https://a.test/" });

  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe("http://x/flows");
  expect(init.method).toBe("POST");
  expect(JSON.parse(init.body)).toEqual({ method: "GET", url: "https://a.test/" });
});

test.each([
  [400, "url must be absolute http(s)"],
  [422, "the seed's request body was not captured (too_large); give a body to replace it"],
  [502, "send failed: tcp connect error: Connection refused (os error 61)"],
  [504, "no response within 30 s"],
])("a %i from either send door throws the core's sentence unwrapped", async (status, sentence) => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status, text: async () => `${sentence}\n` }));

  await expect(createApi("http://x").replay("s", {})).rejects.toThrow(new RegExp(`^${literal(sentence)}$`));
  await expect(createApi("http://x").send({})).rejects.toThrow(new RegExp(`^${literal(sentence)}$`));
});

function literal(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
