import {
  bodyKind,
  bodyNotice,
  bodyText,
  byteLength,
  formEntries,
  formPairCut,
  hexDump,
  hexRows,
  humanBytePair,
  humanBytes,
  rawKind,
  saveFilename,
} from "./body";

test("bodyKind dispatches on the mime type, ignoring parameters", () => {
  expect(bodyKind("application/json")).toBe("json");
  expect(bodyKind("application/json; charset=utf-8")).toBe("json");
  expect(bodyKind("application/problem+json")).toBe("json");
  expect(bodyKind("image/png")).toBe("image");
  expect(bodyKind("application/x-www-form-urlencoded")).toBe("form");
  expect(bodyKind("text/plain; charset=iso-8859-1")).toBe("text");
  expect(bodyKind("application/javascript")).toBe("text");
  expect(bodyKind("application/octet-stream")).toBe("hex");
  expect(bodyKind(null)).toBe("hex");
});

// Six renderers need six kinds: the flat "text" the five-kind contract returned
// for markup left XmlRenderer's dispatch row unimplementable.
test("bodyKind splits the markup sub-group from plain text", () => {
  expect(bodyKind("application/xml")).toBe("xml");
  expect(bodyKind("text/xml")).toBe("xml");
  expect(bodyKind("text/html")).toBe("xml");
  expect(bodyKind("application/soap+xml")).toBe("xml");
  expect(bodyKind("text/html; charset=utf-8")).toBe("xml");
  expect(bodyKind("text/csv")).toBe("text");
});

test("humanBytes reads at a glance: one decimal below 10, integer above", () => {
  expect(humanBytes(0)).toBe("0 B");
  expect(humanBytes(512)).toBe("512 B");
  expect(humanBytes(4096)).toBe("4.0 KiB");
  expect(humanBytes(14_336)).toBe("14 KiB");
  expect(humanBytes(138_240)).toBe("135 KiB");
  expect(humanBytes(1_048_576)).toBe("1.0 MiB");
  expect(humanBytes(15_728_640)).toBe("15 MiB");
});

test("humanBytePair never renders a real cut as two identical sizes", () => {
  for (const [shown, total] of [
    [131_072, 131_083],
    [131_072, 131_073],
    [4096, 4097],
    [1_048_576, 1_048_580],
  ]) {
    const [left, right] = humanBytePair(shown, total);
    expect(left).not.toBe(right);
  }
});

test("humanBytePair borrows no precision when the sizes already differ", () => {
  expect(humanBytePair(4096, 148_480)).toEqual(["4.0 KiB", "145 KiB"]);
  expect(humanBytePair(512, 4096)).toEqual(["512 B", "4.0 KiB"]);
});

test("byteLength counts bytes, not characters — what a cut marker must state", () => {
  expect(byteLength("abc")).toBe(3);
  expect(byteLength("héllo")).toBe(6);
  expect(byteLength("日本")).toBe(6);
});

test("hexRows and hexDump agree — the pane and the clipboard cannot drift", () => {
  const bytes = new TextEncoder().encode("PNG\r\n");
  const [row] = hexRows(bytes);

  expect(hexDump(bytes)).toBe(`${row.offset}  ${row.hex}  ${row.ascii}`);
  expect(row.offset).toBe("00000000");
  expect(row.ascii).toBe("|PNG..|");
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

test("formPairCut lands on a pair boundary, never inside a percent-escape", () => {
  const body = "a=1&price=%E2%82%AC99&b=2";

  for (let at = 0; at <= body.length; at += 1) {
    const kept = body.slice(0, formPairCut(body, at));
    expect(kept).not.toMatch(/%[0-9A-Fa-f]?$/);
    expect(formEntries(kept).map(([, v]) => v).join("")).not.toContain("�");
  }
});

test("formPairCut keeps the key of a first pair that alone exceeds the cap", () => {
  expect("k=".slice(0, formPairCut("k=%E2%82%ACvalue", 4))).toBe("k=");
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
