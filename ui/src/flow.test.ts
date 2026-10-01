import {
  bodyText,
  hostOf,
  isDecryptedHttps,
  lastCaptureAt,
  latestFlowTimes,
  sessionTraffic,
  statusTone,
  toCurl,
  toRow,
} from "./flow";
import type { BodyView, FlowSummary, FlowView } from "./api";

test("status tone: 2xx ok, 4xx/5xx warn, pending/other muted", () => {
  expect(statusTone(200)).toBe("ok");
  expect(statusTone(204)).toBe("ok");
  expect(statusTone(404)).toBe("warn");
  expect(statusTone(500)).toBe("warn");
  expect(statusTone(301)).toBe("muted");
  expect(statusTone(null)).toBe("muted");
});

const flow: FlowSummary = {
  id: "f1",
  started_at: "2026-07-12T10:00:00Z",
  duration_ms: 42,
  method: "GET",
  uri: "http://example.com/a/b?x=1",
  status: 200,
  source: "captured",
  seed: null,
  request_body_state: "empty",
  response_body_state: "captured",
};

test("toRow splits a URI into the host and path cells", () => {
  const row = toRow(flow);
  expect(row.host).toBe("example.com");
  expect(row.path).toBe("/a/b?x=1");
});

test("toRow formats nothing — time, method, status and duration reach their components raw", () => {
  expect(Object.keys(toRow(flow)).sort()).toEqual(["host", "path"]);
});

test("toRow keeps a relative/unparseable URI whole as the path", () => {
  const row = toRow({ ...flow, uri: "/relative/only" });
  expect(row.host).toBe("");
  expect(row.path).toBe("/relative/only");
});

test("isDecryptedHttps: only an https:// URI proves capture", () => {
  expect(isDecryptedHttps([])).toBe(false);
  expect(isDecryptedHttps([flow])).toBe(false); // http://
  expect(isDecryptedHttps([{ ...flow, uri: "/relative/only" }])).toBe(false);
  expect(isDecryptedHttps([{ ...flow, uri: "https://example.com/" }])).toBe(true);
  expect(isDecryptedHttps([flow, { ...flow, id: "f2", uri: "https://example.com/" }])).toBe(true);
});

test("isDecryptedHttps: an imported or sent https row proves nothing — no device routed it", () => {
  const https = { ...flow, uri: "https://example.com/" };
  expect(isDecryptedHttps([{ ...https, source: "imported" }])).toBe(false);
  expect(isDecryptedHttps([{ ...https, source: "sent" }])).toBe(false);
  expect(isDecryptedHttps([{ ...https, source: "sent" }, { ...https, id: "f2" }])).toBe(true);
});

test("isDecryptedHttps: since is the earliest start that counts, and without one any start does", () => {
  const https = { ...flow, uri: "https://example.com/" };
  const at = Date.parse(https.started_at);
  expect(isDecryptedHttps([https], at + 1)).toBe(false);
  expect(isDecryptedHttps([https], at)).toBe(true);
  expect(isDecryptedHttps([https], at - 1)).toBe(true);
  expect(isDecryptedHttps([{ ...https, started_at: "not a date" }])).toBe(true);
  expect(isDecryptedHttps([{ ...https, started_at: "not a date" }], at)).toBe(false);
});

// R4: a second device's setup may not read the first device's requests.
describe("sessionTraffic reads only what started once the session opened", () => {
  const https = { ...flow, id: "f2", uri: "https://example.com/" };
  const at = Date.parse(flow.started_at);

  it("counts a captured row that started at the session's start, and none before it", () => {
    expect(sessionTraffic([flow, https], at, [])).toEqual({
      lastDecryptedAt: at,
      lastPlainAt: at,
      lastTunnelAt: null,
      openTunnelAt: null,
    });
    expect(sessionTraffic([flow, https], at + 1, [])).toEqual({
      lastDecryptedAt: null,
      lastPlainAt: null,
      lastTunnelAt: null,
      openTunnelAt: null,
    });
  });

  it("ignores imported and sent rows inside the session", () => {
    const rows = [{ ...https, source: "imported" as const }, { ...flow, source: "sent" as const }];
    expect(sessionTraffic(rows, at, [])).toMatchObject({ lastDecryptedAt: null, lastPlainAt: null });
  });

  it("keeps a tunnel from the session's start on, and drops an earlier one", () => {
    expect(sessionTraffic([], at, [at])).toMatchObject({ lastTunnelAt: at, openTunnelAt: at });
    expect(sessionTraffic([], at, [at - 1])).toMatchObject({ lastTunnelAt: null, openTunnelAt: null });
  });

  it("bounds nothing before a session has opened", () => {
    expect(sessionTraffic([https], null, [at - 5])).toEqual({
      lastDecryptedAt: at,
      lastPlainAt: null,
      lastTunnelAt: at - 5,
      openTunnelAt: null,
    });
  });

  it("holds open the oldest tunnel no decrypted request started after", () => {
    const tunnels = [at - 10, at + 5, at + 10, at + 15];
    expect(sessionTraffic([https], at - 20, tunnels)).toMatchObject({ lastTunnelAt: at + 15, openTunnelAt: at + 5 });
    expect(sessionTraffic([https], at - 20, [at - 10, at])).toMatchObject({ lastTunnelAt: at, openTunnelAt: null });
    expect(sessionTraffic([flow], at - 20, tunnels)).toMatchObject({ openTunnelAt: at - 10 });
  });
});

test("latestFlowTimes: since is the earliest start that counts", () => {
  const at = Date.parse(flow.started_at);
  expect(latestFlowTimes([flow], at).lastPlainAt).toBe(at);
  expect(latestFlowTimes([flow], at + 1).lastPlainAt).toBeNull();
});

const b64 = (s: string) => btoa(String.fromCharCode(...new TextEncoder().encode(s)));

function view(over: Partial<FlowView> = {}): FlowView {
  return {
    id: "f1",
    started_at: "2026-07-12T10:00:00Z",
    duration_ms: 5,
    method: "GET",
    uri: "http://example.com/a?x=1",
    status: 200,
    source: "captured",
    seed: null,
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/1.1",
    request_headers: [{ name: "Accept", value: "application/json" }],
    response_headers: [],
    request_body: { state: "empty" },
    response_body: { state: "empty" },
    ...over,
  };
}

test("bodyText decodes captured base64 as UTF-8, empty for non-captured", () => {
  expect(bodyText({ state: "captured", size: 3, data_base64: b64("héllo") })).toBe("héllo");
  expect(bodyText({ state: "empty" })).toBe("");
  expect(bodyText({ state: "streaming", content_type: "text/event-stream" })).toBe("");
});

test("toCurl omits -X for GET, quotes url + each header", () => {
  expect(toCurl(view())).toBe(
    "curl 'http://example.com/a?x=1' -H 'Accept: application/json'",
  );
});

test("toCurl adds method and --data-raw for a captured request body", () => {
  const c = toCurl(
    view({
      method: "POST",
      request_body: { state: "captured", size: 9, data_base64: b64('{"a":1}') },
    }),
  );
  expect(c).toContain("-X POST");
  expect(c).toContain("--data-raw '{\"a\":1}'");
});

test("toCurl escapes embedded single quotes in a shell-safe way", () => {
  const body: BodyView = { state: "captured", size: 3, data_base64: b64("it's") };
  const c = toCurl(view({ method: "POST", request_body: body }));
  expect(c).toContain("--data-raw 'it'\\''s'");
});

// The setup screen judges "now" off these, so the time is each flow's own
// started_at — never when the list arrived — and a flow that cannot be dated
// contributes nothing rather than a NaN that would poison the max.
test("latestFlowTimes takes the newest started_at per kind, decrypted apart from plain", () => {
  const at = (iso: string, uri: string): FlowSummary => ({ ...flow, started_at: iso, uri });

  expect(
    latestFlowTimes([
      at("2026-07-12T10:00:00Z", "https://a.test/"),
      at("2026-07-12T10:00:05Z", "http://b.test/"),
      at("2026-07-12T10:00:03Z", "https://c.test/"),
      at("not a date", "https://d.test/"),
    ]),
  ).toEqual({
    lastDecryptedAt: Date.parse("2026-07-12T10:00:03Z"),
    lastPlainAt: Date.parse("2026-07-12T10:00:05Z"),
  });
  expect(latestFlowTimes([])).toEqual({ lastDecryptedAt: null, lastPlainAt: null });
});

test("latestFlowTimes reads captured rows only, of either scheme", () => {
  const at = (source: FlowSummary["source"], uri: string): FlowSummary => ({ ...flow, source, uri });
  expect(
    latestFlowTimes([
      at("imported", "https://a.test/"),
      at("sent", "https://b.test/"),
      at("imported", "http://c.test/"),
      at("sent", "http://d.test/"),
    ]),
  ).toEqual({ lastDecryptedAt: null, lastPlainAt: null });
});

// Q4: plain http counts for the dot, which asks only whether the proxy is in the path.
test("lastCaptureAt is the newest captured row of either scheme, and null with none", () => {
  const at = (iso: string, uri: string, source: FlowSummary["source"] = "captured"): FlowSummary => ({
    ...flow,
    started_at: iso,
    uri,
    source,
  });

  expect(lastCaptureAt([at("2026-07-12T10:00:00Z", "https://a.test/"), at("2026-07-12T10:00:05Z", "http://b.test/")])).toBe(
    Date.parse("2026-07-12T10:00:05Z"),
  );
  expect(lastCaptureAt([at("2026-07-12T10:00:00Z", "http://a.test/")])).toBe(Date.parse("2026-07-12T10:00:00Z"));
  expect(lastCaptureAt([at("2026-07-12T10:00:09Z", "https://a.test/")])).toBe(Date.parse("2026-07-12T10:00:09Z"));
  expect(lastCaptureAt([at("2026-07-12T10:00:09Z", "https://a.test/", "sent")])).toBeNull();
  expect(lastCaptureAt([])).toBeNull();
});

// hostOf answers from a memo keyed on the URI's authority, so what matters is
// that it is indistinguishable from the parse it stands in for — including on
// the URIs whose authority the shortcut does not recognise.
describe("hostOf", () => {
  const cases = [
    "https://api.example.com/v2/thing?page=2",
    "https://api.example.com/v2/other",
    "https://api.example.com",
    "https://api.example.com:8443/x",
    "https://api.example.com:443/x",
    "http://api.example.com:80/x",
    "https://user:secret@api.example.com/x",
    "http://[2001:db8::1]:8080/x",
    "https://XN--e1afmkfd.xn--p1ai/x",
    "https://api.example.com?only=query",
    "https://api.example.com#only-fragment",
    "http:example.com/no-slashes",
  ];

  it("returns exactly what a full URL parse returns", () => {
    for (const uri of cases) {
      expect(hostOf(uri)).toBe(new URL(uri).host);
    }
  });

  it("agrees with toRow, which parses the whole URI for its path", () => {
    for (const uri of cases) {
      expect(hostOf(uri)).toBe(toRow({ ...flow, uri }).host);
    }
  });

  it("names no host for a URI that has none, rather than inventing one", () => {
    expect(hostOf("/relative/path")).toBe("");
    expect(hostOf("mailto:someone@example.com")).toBe("");
    expect(hostOf("")).toBe("");
  });

  it("is stable across repeats — a second ask is the same answer, not a fresh guess", () => {
    expect(hostOf("https://repeat.example.com/one")).toBe("repeat.example.com");
    expect(hostOf("https://repeat.example.com/two")).toBe("repeat.example.com");
    expect(hostOf("https://repeat.example.com/one")).toBe("repeat.example.com");
  });

  // The memo is bounded, and the bound is a size limit on a cache — never a
  // limit on what merge can accumulate.
  it("keeps answering correctly past its own memo bound", () => {
    for (let i = 0; i < 2500; i += 1) {
      expect(hostOf(`https://h${i}.example.com/x`)).toBe(`h${i}.example.com`);
    }
    expect(hostOf("https://h0.example.com/y")).toBe("h0.example.com");
  });
});
