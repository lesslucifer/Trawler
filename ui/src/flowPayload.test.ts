import { describe, expect, it } from "vitest";
import type { FlowView } from "./api";
import { bodyPayload } from "./body";
import { headersPayload, uriPayload } from "./flowPayload";

const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

function view(over: Partial<FlowView> = {}): FlowView {
  return {
    id: "f7",
    started_at: "2026-11-05T10:00:00Z",
    duration_ms: 3,
    method: "GET",
    uri: "https://api.example.com/v2/accounts/8812/sessions?page=2&sort=desc",
    status: 200,
    source: "captured",
    seed: null,
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/1.1",
    request_headers: [
      { name: "Host", value: "api.example.com" },
      { name: "Authorization", value: "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI0MiJ9.c2ln" },
    ],
    response_headers: [
      { name: "Set-Cookie", value: "a=1; Path=/" },
      { name: "Content-Type", value: "text/plain; charset=utf-8" },
      { name: "Set-Cookie", value: "b=2; Expires=Thu, 01 Jan 2026 00:00:00 GMT" },
    ],
    request_body: { state: "empty" },
    response_body: { state: "empty" },
    ...over,
  };
}

describe("uriPayload — what the URI's open hands a data window", () => {
  it("the whole URI, keyed by the flow alone, with no type hint", () => {
    const payload = uriPayload(view());

    expect(payload.key).toBe("f7:uri");
    expect(text(payload.bytes)).toBe("https://api.example.com/v2/accounts/8812/sessions?page=2&sort=desc");
    expect("typeHint" in payload).toBe(false);
  });

  it("a 2 KiB URI goes whole, byte for byte", () => {
    const uri = `https://cdn.example.com/a?${"q=é".repeat(700)}`;

    expect(uriPayload(view({ uri })).bytes).toEqual(new TextEncoder().encode(uri));
  });

  it("is titled and headed like the body's, with uri as what", () => {
    const payload = uriPayload(view());

    expect(payload.title).toBe("uri · GET api.example.com/v2/accounts/8812/sessions?page=2&sort=desc");
    expect(payload.heading).toEqual({
      what: "uri",
      method: "GET",
      host: "api.example.com",
      path: "/v2/accounts/8812/sessions",
      query: "?page=2&sort=desc",
    });
  });
});

describe("headersPayload — what open all hands a data window", () => {
  it("one Name: value line per header, in wire order, duplicate names kept apart", () => {
    expect(text(headersPayload(view(), "response").bytes)).toBe(
      [
        "Set-Cookie: a=1; Path=/",
        "Content-Type: text/plain; charset=utf-8",
        "Set-Cookie: b=2; Expires=Thu, 01 Jan 2026 00:00:00 GMT",
      ].join("\n"),
    );
  });

  it("keys by flow and leg, so each leg opens its own window and a second open focuses it", () => {
    const flow = view();

    expect(headersPayload(flow, "request").key).toBe("f7:request:headers");
    expect(headersPayload(flow, "response").key).toBe("f7:response:headers");
    expect(headersPayload(flow, "request").key).toBe(headersPayload(view(), "request").key);
  });

  it("values go byte for byte: a colon inside a value, non-ASCII, a 4 KiB cookie", () => {
    const cookie = `session=${"x".repeat(4096)}`;
    const flow = view({
      request_headers: [
        { name: "X-Note", value: "a: b: c" },
        { name: "X-Name", value: "Đặng — ☃" },
        { name: "Cookie", value: cookie },
      ],
    });

    expect(text(headersPayload(flow, "request").bytes)).toBe(`X-Note: a: b: c\nX-Name: Đặng — ☃\nCookie: ${cookie}`);
  });

  it("an empty leg gives no lines", () => {
    expect(headersPayload(view({ request_headers: [] }), "request").bytes).toHaveLength(0);
  });

  it("names its leg in the title and the heading, and sends no type hint", () => {
    const payload = headersPayload(view(), "request");

    expect(payload.title).toBe("request headers · GET api.example.com/v2/accounts/8812/sessions?page=2&sort=desc");
    expect(payload.heading?.what).toBe("request headers");
    expect("typeHint" in payload).toBe(false);
  });

  it("splits the URI exactly as the body's payload does", () => {
    const flow = view({ uri: "http://localhost:8080" });
    const body = bodyPayload(flow, "request", { bytes: new Uint8Array(), contentType: null }, true);

    expect(headersPayload(flow, "request").heading).toEqual({ ...body.heading, what: "request headers" });
    expect(uriPayload(flow).heading).toEqual({ ...body.heading, what: "uri" });
  });
});
