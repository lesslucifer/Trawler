import {
  bodyKind,
  bodyNotice,
  bodyText,
  formEntries,
  hexDump,
  rawKind,
  saveFilename,
} from "./body";

test("bodyKind dispatches on the mime type, ignoring parameters", () => {
  expect(bodyKind("application/json")).toBe("json");
  expect(bodyKind("application/json; charset=utf-8")).toBe("json");
  expect(bodyKind("application/problem+json")).toBe("json");
  expect(bodyKind("image/png")).toBe("image");
  expect(bodyKind("application/x-www-form-urlencoded")).toBe("form");
  expect(bodyKind("text/html")).toBe("text");
  expect(bodyKind("text/plain; charset=iso-8859-1")).toBe("text");
  expect(bodyKind("application/xml")).toBe("text");
  expect(bodyKind("application/soap+xml")).toBe("text");
  expect(bodyKind("application/javascript")).toBe("text");
  expect(bodyKind("application/octet-stream")).toBe("hex");
  expect(bodyKind(null)).toBe("hex");
});

test("rawKind sniffs stored bytes: printable UTF-8 is text, anything else hex", () => {
  expect(rawKind(new TextEncoder().encode('{"a":1}\n\ttab'))).toBe("text");
  expect(rawKind(new TextEncoder().encode("héllo"))).toBe("text");
  expect(rawKind(new Uint8Array([0x00, 0x01, 0x02]))).toBe("hex");
  expect(rawKind(new Uint8Array([0xff, 0xfe]))).toBe("hex");
});

test("hexDump formats offset + hex pairs + ascii gutter, 16 bytes per line", () => {
  const bytes = new TextEncoder().encode("GET / HTTP/1.1\r\nHost: x");
  const lines = hexDump(bytes).split("\n");
  expect(lines[0]).toBe("00000000  47 45 54 20 2f 20 48 54 54 50 2f 31 2e 31 0d 0a  |GET / HTTP/1.1..|");
  expect(lines[1]).toBe("00000010  48 6f 73 74 3a 20 78                             |Host: x|");
});

test("hexDump caps output and reports the remainder", () => {
  const lines = hexDump(new Uint8Array(40), 16).split("\n");
  expect(lines).toHaveLength(2);
  expect(lines[1]).toBe("… 24 more bytes");
});

test("formEntries parses url-encoded pairs, decoding and keeping duplicates", () => {
  expect(formEntries("a=1&b=two%20words&a=2")).toEqual([
    ["a", "1"],
    ["b", "two words"],
    ["a", "2"],
  ]);
});

test("bodyNotice maps every non-captured state to a notice, captured to null", () => {
  expect(bodyNotice({ state: "captured", size: 3, data_base64: "aGk=" })).toBeNull();
  expect(bodyNotice(null)).toBe("no response yet");
  expect(bodyNotice({ state: "empty" })).toBe("empty body");
  expect(bodyNotice({ state: "streaming", content_type: "text/event-stream" })).toContain(
    "streaming (text/event-stream)",
  );
  expect(bodyNotice({ state: "not_captured", reason: "too_large", actual: 42 })).toBe(
    "not captured — too large (42 bytes)",
  );
  expect(bodyNotice({ state: "not_captured", reason: "read_error", actual: null })).toBe(
    "not captured — read error",
  );
});

test("saveFilename derives short-id, leg, and a content-type extension", () => {
  const id = "0192a1b2-c3d4-7e8f-9012-3456789abcde";
  expect(saveFilename(id, "response", "application/json; charset=utf-8")).toBe(
    "0192a1b2-response.json",
  );
  expect(saveFilename(id, "request", "application/x-www-form-urlencoded")).toBe(
    "0192a1b2-request.txt",
  );
  expect(saveFilename(id, "response", "image/png")).toBe("0192a1b2-response.png");
  expect(saveFilename(id, "response", "image/webp")).toBe("0192a1b2-response.webp");
  expect(saveFilename(id, "response", "image/avif")).toBe("0192a1b2-response.avif");
  expect(saveFilename(id, "response", "image/svg+xml")).toBe("0192a1b2-response.svg");
  expect(saveFilename(id, "response", "text/html")).toBe("0192a1b2-response.html");
  expect(saveFilename(id, "response", null)).toBe("0192a1b2-response.bin");
});

test("bodyText decodes bytes as UTF-8", () => {
  expect(bodyText(new TextEncoder().encode("héllo"))).toBe("héllo");
});
