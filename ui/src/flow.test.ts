import { bodyText, statusTone, toCurl, toRow } from "./flow";
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
  request_body_state: "empty",
  response_body_state: "captured",
};

test("toRow splits host/path and formats status + duration", () => {
  const row = toRow(flow);
  expect(row.host).toBe("example.com");
  expect(row.path).toBe("/a/b?x=1");
  expect(row.method).toBe("GET");
  expect(row.status).toBe("200");
  expect(row.tone).toBe("ok");
  expect(row.duration).toBe("42 ms");
});

test("toRow shows placeholders for a pending flow", () => {
  const row = toRow({ ...flow, status: null, duration_ms: null });
  expect(row.status).toBe("…");
  expect(row.tone).toBe("muted");
  expect(row.duration).toBe("");
});

test("toRow keeps a relative/unparseable URI whole as the path", () => {
  const row = toRow({ ...flow, uri: "/relative/only" });
  expect(row.host).toBe("");
  expect(row.path).toBe("/relative/only");
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
