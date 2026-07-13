import { afterEach, vi } from "vitest";
import { createApi, filterParams } from "./api";
import type { FlowSummary } from "./api";

afterEach(() => vi.restoreAllMocks());

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

test("listFlows throws on a non-ok response", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 500 }));
  await expect(createApi("http://x").listFlows({})).rejects.toThrow(/500/);
});

test("getBody fetches the leg body, decode=1 only when asked, and keeps the content type", async () => {
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

  await api.getBody("f1", "request", false);
  expect(fetchMock).toHaveBeenLastCalledWith("http://x/flows/f1/request/body");
});

test("getBody throws on a non-ok response", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 404 }));
  await expect(createApi("http://x").getBody("f1", "response", true)).rejects.toThrow(/404/);
});
