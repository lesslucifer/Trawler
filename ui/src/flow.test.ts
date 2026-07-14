import { isDecryptedHttps, statusTone, toRow } from "./flow";
import type { FlowSummary } from "./api";

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

test("isDecryptedHttps: only an https:// URI proves capture", () => {
  expect(isDecryptedHttps([])).toBe(false);
  expect(isDecryptedHttps([flow])).toBe(false); // http://
  expect(isDecryptedHttps([{ ...flow, uri: "/relative/only" }])).toBe(false);
  expect(isDecryptedHttps([{ ...flow, uri: "https://example.com/" }])).toBe(true);
  expect(isDecryptedHttps([flow, { ...flow, id: "f2", uri: "https://example.com/" }])).toBe(true);
});
