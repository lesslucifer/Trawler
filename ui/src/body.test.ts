import {
  bodyKind,
  bodyPayload,
  captionFilename,
  bodyNotice,
  bodyText,
  byteLength,
  decodeBody,
  formEntries,
  formPairCut,
  hexDump,
  hexRows,
  isFullPath,
  humanBytePair,
  humanBytes,
  rawKind,
  saveFilename,
} from "./body";
import type { FlowView } from "./api";

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

// W7's C1: a command names the file where the save put it, so the dialog opens
// in the folder the command names until the user points it elsewhere.
test("saveBody opens the dialog in the folder it is given, and where it would anyway when that folder cannot be read", async () => {
  const bytes = new TextEncoder().encode("pem");
  vi.stubGlobal("window", { ...globalThis.window, __TAURI_INTERNALS__: {} });
  const save = vi.fn(async () => null);
  vi.doMock("@tauri-apps/plugin-dialog", () => ({ save }));
  vi.doMock("@tauri-apps/plugin-fs", () => ({ writeFile: vi.fn(async () => {}) }));
  vi.doMock("@tauri-apps/api/path", () => ({ join: vi.fn(async (...parts: string[]) => parts.join("/")) }));
  vi.resetModules();
  const { saveBody: saving } = await import("./body");

  await saving(bytes, "trawler-ca.pem", async () => "/Users/me/Downloads");
  expect(save).toHaveBeenLastCalledWith({ defaultPath: "/Users/me/Downloads/trawler-ca.pem" });
  await saving(bytes, "trawler-ca.pem", () => Promise.reject(new Error("no such folder")));
  expect(save).toHaveBeenLastCalledWith({ defaultPath: "trawler-ca.pem" });
  await saving(bytes, "body.json");
  expect(save).toHaveBeenLastCalledWith({ defaultPath: "body.json" });
});

test("a full path is what a native save returns, never a browser download's bare name", () => {
  for (const path of ["/Users/me/trawler-ca.pem", "C:\\Users\\me\\trawler-ca.pem", "D:/certs/trawler-ca.pem", "\\\\host\\share\\trawler-ca.pem"])
    expect(isFullPath(path)).toBe(true);
  for (const path of ["trawler-ca.pem", "certs/trawler-ca.pem", "~/Downloads/trawler-ca.pem"])
    expect(isFullPath(path)).toBe(false);
});

// openFile is saveBody's inverse and carries the same contract, so it gets the
// same proof: a dismissed picker is an outcome, not a failure, and the caller
// must be able to tell the two apart. Found untested by review round 1.
test("openFile reads the picked file, and answers null when the native dialog was dismissed", async () => {
  vi.stubGlobal("window", { ...globalThis.window, __TAURI_INTERNALS__: {} });
  const bytes = new Uint8Array([1, 2, 3]);
  const readFile = vi.fn(async () => bytes);
  vi.doMock("@tauri-apps/plugin-fs", () => ({ readFile }));

  vi.doMock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
  vi.resetModules();
  expect(await (await import("./body")).openFile(["har"])).toBeNull();
  expect(readFile).not.toHaveBeenCalled();

  const open = vi.fn(async () => "/tmp/session.har");
  vi.doMock("@tauri-apps/plugin-dialog", () => ({ open }));
  vi.resetModules();
  const chosen = await (await import("./body")).openFile(["har", "json"]);
  expect(chosen).toEqual({ name: "/tmp/session.har", bytes });
  expect(readFile).toHaveBeenCalledWith("/tmp/session.har");
  // Single-file, not a directory: the import door takes one document.
  expect(open).toHaveBeenCalledWith(
    expect.objectContaining({ multiple: false, directory: false }),
  );
});

// The browser half runs under vite dev and here. Its cancel listener is the
// load-bearing part: a dismissed picker fires no change event, so without it the
// caller awaits a promise that never settles — and the import dialog would hang
// disabled with no way back.
test("openFile's browser fallback settles on a chosen file and on a dismissal alike", async () => {
  vi.stubGlobal("window", {});
  vi.resetModules();
  const { openFile } = await import("./body");

  const inputs: HTMLInputElement[] = [];
  const create = document.createElement.bind(document);
  vi.spyOn(document, "createElement").mockImplementation((tag: string) => {
    const el = create(tag) as HTMLInputElement;
    if (tag === "input") {
      el.click = () => {};
      inputs.push(el);
    }
    return el;
  });

  const dismissed = openFile(["har"]);
  inputs[0].dispatchEvent(new Event("cancel"));
  expect(await dismissed).toBeNull();

  const picked = openFile(["har", "json"]);
  const input = inputs[1];
  expect(input.accept).toBe(".har,.json");
  Object.defineProperty(input, "files", {
    value: [{ name: "session.har", arrayBuffer: async () => new Uint8Array([7, 8]).buffer }],
  });
  input.dispatchEvent(new Event("change"));
  expect(await picked).toEqual({ name: "session.har", bytes: new Uint8Array([7, 8]) });

  // A change event carrying no file is a dismissal too, not a hang.
  const empty = openFile(["har"]);
  Object.defineProperty(inputs[2], "files", { value: [] });
  inputs[2].dispatchEvent(new Event("change"));
  expect(await empty).toBeNull();

  // A read that fails rejects rather than never settling. The caller disables
  // its controls for the whole wait, so a hung promise is a dialog with no way
  // forward and no way back.
  const unreadable = openFile(["har"]);
  Object.defineProperty(inputs[3], "files", {
    value: [{ name: "gone.har", arrayBuffer: async () => Promise.reject(new Error("no longer there")) }],
  });
  inputs[3].dispatchEvent(new Event("change"));
  await expect(unreadable).rejects.toThrow("no longer there");
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
  });});

describe("bodyPayload — what the pane hands a data window", () => {
  const view = {
    id: "f1",
    method: "POST",
    uri: "https://auth.example.com/oauth/token",
  } as FlowView;
  const fetched = { bytes: new Uint8Array([1, 2]), contentType: "application/x-www-form-urlencoded" };

  it("pretty: the leg, method and where in the title, the served type as the hint", () => {
    expect(bodyPayload(view, "request", fetched, true)).toEqual({
      key: "f1:request:pretty",
      title: "request body · POST auth.example.com/oauth/token",
      bytes: fetched.bytes,
      typeHint: "application/x-www-form-urlencoded",
    });
  });

  it("raw: a key of its own, said in the title, and no hint", () => {
    const payload = bodyPayload(view, "request", fetched, false);
    expect(payload.key).toBe("f1:request:raw");
    expect(payload.title).toBe("request body · raw · POST auth.example.com/oauth/token");
    expect("typeHint" in payload).toBe(false);
  });

  it("no served type, no hint", () => {
    expect("typeHint" in bodyPayload(view, "response", { ...fetched, contentType: null }, true)).toBe(false);
  });
});

describe("captionFilename — a data window saves under its opener's caption", () => {
  it("the caption made file-safe, the hint's extension, an optional suffix", () => {
    expect(captionFilename("response body · GET api.example.com/v2/x", "application/json")).toBe(
      "response-body-GET-api.example.com-v2-x.json",
    );
    expect(captionFilename("a b", undefined, "-original")).toBe("a-b-original.bin");
  });

  it("a caption with nothing usable is data; a long one is cut short", () => {
    expect(captionFilename("· · ·", "text/plain")).toBe("data.txt");
    expect(captionFilename("x".repeat(300), "text/plain")).toBe(`${"x".repeat(80)}.txt`);
  });
});

describe("byteLength — one UTF-8 length rule, counted without encoding", () => {
  it.each(["", "abc", "café", "日本", "a😀b", "\ud800", "\udc00x", "\ud800\u0800", "\ud800é", "\ud800a", "x\ud800"])(
    "%j matches TextEncoder",
    (text) => {
      expect(byteLength(text)).toBe(new TextEncoder().encode(text).length);
    },
  );
});
