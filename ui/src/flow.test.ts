import { bodyText, isDecryptedHttps, latestFlowTimes, statusTone, toCurl, toRow } from "./flow";
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
