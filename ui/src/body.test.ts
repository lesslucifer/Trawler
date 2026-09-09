import {
  bodyKind,
  bodyNotice,
  bodyText,
  byteLength,
  decodeBody,
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
  expect("k=%E2%82%ACvalue".slice(0, formPairCut("k=%E2%82%ACvalue", 4))).toBe("k=");
});

// A cut that returns more than it was asked for defeats the cap it exists to
// enforce — a key longer than the cap is the case that does it. Returning the
// whole length is the worse shape: the caller then draws a marker claiming a
// truncation that did not happen.
test("formPairCut never returns past the cap it was given", () => {
  for (const body of ["averyveryverylongkey=v", "noequalsnoamp", "&a=1", "k=v", "", "a=1&b=2"]) {
    for (let at = 0; at <= body.length + 2; at += 1) {
      const cut = formPairCut(body, at);
      expect(cut).toBeLessThanOrEqual(at);
      if (body.length > at) expect(body.slice(0, cut)).not.toBe(body);
    }
  }
});

test("bodyNotice maps every non-captured state to a notice, captured to null", () => {
  expect(bodyNotice({ state: "captured", size: 3, data_base64: "aGk=" })).toBeNull();
  expect(bodyNotice(null)).toBe("no response yet");
  expect(bodyNotice({ state: "empty" })).toBe("empty body");
  expect(bodyNotice({ state: "streaming", content_type: "text/event-stream" })).toContain(
    "streaming (text/event-stream)",
  );
  expect(bodyNotice({ state: "not_captured", reason: "read_error", actual: null })).toBe(
    "not captured — read error",
  );
});

// The notice and a CutMarker can sit three lines apart in one pane, and the
// marker's @avoid forbids raw byte counts — so this sentence states the same
// kind of size the marker does.
test("the too-large notice states a human size, never a raw byte count", () => {
  expect(bodyNotice({ state: "not_captured", reason: "too_large", actual: 42 })).toBe(
    "not captured — too large (42 B)",
  );
  expect(bodyNotice({ state: "not_captured", reason: "too_large", actual: 12_994_392 })).toBe(
    "not captured — too large (12 MiB)",
  );
});

// A size the server did not report is not zero: "0 B" would be a claim the
// notice cannot make, so the sentence drops the parenthetical instead.
test("the too-large notice omits the size when the server reported none", () => {
  expect(bodyNotice({ state: "not_captured", reason: "too_large", actual: null })).toBe(
    "not captured — too large",
  );
});

test("saveFilename names the endpoint, the leg, and the served type", () => {
  const uri = "https://api.stripe.com/v1/charges";
  expect(saveFilename(uri, "response", "application/json; charset=utf-8")).toBe(
    "charges-response.json",
  );
  expect(saveFilename(uri, "request", "application/x-www-form-urlencoded")).toBe(
    "charges-request.txt",
  );
  expect(saveFilename(uri, "response", "image/png")).toBe("charges-response.png");
  expect(saveFilename(uri, "response", "image/svg+xml")).toBe("charges-response.svg");
  expect(saveFilename(uri, "response", "text/html")).toBe("charges-response.html");
  expect(saveFilename(uri, "response", "application/javascript")).toBe("charges-response.js");
  expect(saveFilename(uri, "response", "text/css")).toBe("charges-response.css");
  expect(saveFilename(uri, "response", null)).toBe("charges-response.bin");
});

// The id told the reader nothing in a Downloads folder; the endpoint does.
test("saveFilename falls back to the host when the path names nothing", () => {
  expect(saveFilename("https://example.com/", "response", "text/html")).toBe(
    "example.com-response.html",
  );
  expect(saveFilename("https://example.com", "response", "text/html")).toBe(
    "example.com-response.html",
  );
});

test("saveFilename does not stack a second extension on a segment that has one", () => {
  expect(saveFilename("https://cdn.example.com/static/app.js", "response", "application/javascript")).toBe(
    "app-response.js",
  );
});

// A filename that is only separators, or that leads with one, reads as broken
// and is hostile to a shell — every degenerate URI must still land on a name.
test("saveFilename never emits a leading separator or an empty name", () => {
  for (const uri of [
    "https://x.com/%",
    "https://x.com/%zz",
    "https://x.com/a%E0%A4%A",
    "https://x.com/...",
    "https://x.com/---",
    "",
    "/",
    "https://x.com/.gitignore",
  ]) {
    const name = saveFilename(uri, "response", "text/plain");
    expect(name).not.toMatch(/^[-._]/);
    expect(name.endsWith("-response.txt")).toBe(true);
    expect(name.length).toBeGreaterThan("-response.txt".length);
  }
});

// A wholly non-ASCII path sanitizes to nothing; the punycode host still names
// the endpoint, which beats a generic fallback.
test("saveFilename falls back to the host for an IDN path, not to 'body'", () => {
  expect(saveFilename("https://\u4f8b\u3048.jp/\u9053", "response", "text/html")).toBe(
    "xn--r8jz45g.jp-response.html",
  );
});

test("saveFilename drops the query and anything a filename cannot carry", () => {
  expect(saveFilename("https://example.com/a b/rep ort?x=1&y=2", "response", "text/csv")).toBe(
    "rep-ort-response.csv",
  );
  expect(saveFilename("not a url at all", "response", "text/plain")).toBe(
    "not-a-url-at-all-response.txt",
  );
});

test("bodyText decodes bytes as UTF-8", () => {
  expect(bodyText(new TextEncoder().encode("héllo"))).toBe("héllo");
});

// The native dialog resolves to null when the user cancels, which is
// indistinguishable from a completed write unless saveBody says which happened.
// Onboarding claims "saved as trawler-ca.pem" on the strength of this answer and
// then tells the user to run commands against that file, so a cancel reported as
// a save sends them to a terminal with a file that was never written.
test("saveBody reports the path it wrote, and null when the native dialog was cancelled", async () => {
  const bytes = new TextEncoder().encode("pem");
  vi.stubGlobal("window", { ...globalThis.window, __TAURI_INTERNALS__: {} });

  vi.doMock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn(async () => null) }));
  const writeFile = vi.fn(async () => {});
  vi.doMock("@tauri-apps/plugin-fs", () => ({ writeFile }));
  vi.resetModules();
  const cancelled = await (await import("./body")).saveBody(bytes, "trawler-ca.pem");
  expect(cancelled).toBeNull();
  expect(writeFile).not.toHaveBeenCalled();

  vi.doMock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn(async () => "/tmp/picked.pem") }));
  vi.resetModules();
  const written = await (await import("./body")).saveBody(bytes, "trawler-ca.pem");
  expect(written).toBe("/tmp/picked.pem");
  // The returned path is a claim that the bytes landed there; assert the write
  // itself, or a refactor that reports the path without writing passes.
  expect(writeFile).toHaveBeenCalledWith("/tmp/picked.pem", bytes);
});

describe("decodeBody", () => {
  it("reports a whole decode, and returns what the lenient decoder returns", () => {
    const bytes = new TextEncoder().encode("résumé — naïve");

    expect(decodeBody(bytes)).toEqual({ text: bodyText(bytes), whole: true });
  });

  it("calls a body whole when it carries a replacement character of its own", () => {
    const bytes = new TextEncoder().encode("lost: \uFFFD");

    expect(decodeBody(bytes)).toEqual({ text: "lost: \uFFFD", whole: true });
    expect(byteLength(bodyText(bytes))).toBe(bytes.length);
  });

  it("reports a lossy decode, and still returns the text to render", () => {
    const bytes = new Uint8Array([0x68, 0x69, 0xff, 0xfe]);
    const decoded = decodeBody(bytes);

    expect(decoded.whole).toBe(false);
    expect(decoded.text).toBe(bodyText(bytes));
    expect(byteLength(decoded.text)).toBeGreaterThan(bytes.length);
  });

  // A mark is consumed rather than decoded, so the text comes out shorter than
  // the bytes without a single one being replaced. The body's size stays its
  // bytes' — that is what a save writes — and the text's is no longer it.
  it("calls a body carrying a byte-order mark whole, though the mark leaves its text", () => {
    const bytes = new Uint8Array([0xef, 0xbb, 0xbf, ...new TextEncoder().encode("hi")]);
    const decoded = decodeBody(bytes);

    expect(decoded).toEqual({ text: "hi", whole: true });
    expect(byteLength(decoded.text)).toBe(bytes.length - 3);
  });

  it("reports an empty body whole — there is no byte to fail on", () => {
    expect(decodeBody(new Uint8Array())).toEqual({ text: "", whole: true });
  });
});
