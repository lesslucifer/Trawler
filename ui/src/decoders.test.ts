import { describe, expect, it } from "vitest";
import {
  base64,
  contentKey,
  decode,
  decodeAround,
  guessDecoder,
  jwt,
  opensAs,
  replacement,
  url,
  type Decoded,
  type Reading,
} from "./decoders";
import { spliceAsJson } from "./jsonTools";

const b64 = (bytes: Uint8Array | string) =>
  btoa(String.fromCharCode(...(typeof bytes === "string" ? new TextEncoder().encode(bytes) : bytes)));
const b64url = (text: string) => b64(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const token = (header: string, payload: string, signature = "c2ln") => `${b64url(header)}.${b64url(payload)}.${signature}`;
const GZIP = Uint8Array.of(0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03);
const PROTOBUF = Uint8Array.of(0x08, 0x96, 0x01, 0x12, 0x0b, 0x73, 0x75, 0x67, 0x67, 0x65, 0x73, 0x74, 0x69, 0x6f, 0x6e, 0x73, 0x18, 0xfe, 0x03);

const listed = (bytes: Uint8Array | string | null) =>
  bytes === null ? null : Array.from(typeof bytes === "string" ? new TextEncoder().encode(bytes) : bytes);

function decoded(reading: Reading): Decoded {
  if (reading.kind === "unreadable") throw new Error(reading.reason);
  return reading;
}

describe("base64", () => {
  it("reads standard, URL-safe, padded and padding-less input alike, whitespace ignored", () => {
    const standard = b64("subjects?>>"); // carries + and /
    expect(standard).toMatch(/[+/]/);
    for (const input of [standard, standard.replace(/=+$/, ""), standard.replace(/\+/g, "-").replace(/\//g, "_"), ` ${standard.slice(0, 4)}\n${standard.slice(4)} `]) {
      expect(decoded(base64(input))).toMatchObject({ kind: "text", text: "subjects?>>" });
    }
  });

  it("says why it cannot read a length of 4n + 1, a foreign character or padding mid-string", () => {
    expect(base64("abcde")).toEqual({ kind: "unreadable", reason: "5 characters cannot be base64: a length of 4n + 1 never decodes" });
    expect(base64("ab*d")).toEqual({ kind: "unreadable", reason: "character 3, “*”, is not in the base64 alphabet" });
    expect(base64("ab=cd")).toEqual({ kind: "unreadable", reason: "“=” at character 3 is padding, which only ends base64" });
    expect(base64("==")).toEqual({ kind: "unreadable", reason: "it decodes to nothing" });
  });

  it("lays JSON out at 2 spaces and keeps a number past 2^53 as written", () => {
    const reading = decoded(base64(b64('{"id":9007199254740993,"a":[1]}')));
    expect(reading).toMatchObject({ kind: "json", text: '{"id":9007199254740993,"a":[1]}' });
    expect(reading.kind === "json" && reading.shown).toBe('{\n  "id": 9007199254740993,\n  "a": [\n    1\n  ]\n}');
  });

  it("reads bytes that are not UTF-8 text as binary, naming a known signature", () => {
    expect(decoded(base64(b64(GZIP)))).toEqual({ kind: "binary", bytes: GZIP, looks: "gzip" });
    expect(decoded(base64(b64(Uint8Array.of(0x00, 0xfe, 0x7f))))).toMatchObject({ kind: "binary", looks: null });
    expect(decoded(base64(b64("bell\u0007")))).toMatchObject({ kind: "binary" });
    expect(decoded(base64(b64("tab\tand\r\nbreak")))).toMatchObject({ kind: "text" });
  });
});

describe("text or binary is the body's own rule (rawKind)", () => {
  it("DEL is text and a vertical tab or form feed is binary, as the window that opens them reads them", () => {
    expect(decoded(url("a%7Fb"))).toMatchObject({ kind: "text", text: "a\u007fb" });
    expect(decoded(url("a%0Bb"))).toMatchObject({ kind: "binary" });
    expect(decoded(url("a%0Cb"))).toMatchObject({ kind: "binary" });
  });
});

describe("url", () => {
  it("decodes escapes and + as a space", () => {
    expect(decoded(url("a%20b+c%26d%E2%9C%93"))).toMatchObject({ kind: "text", text: "a b c&d✓" });
  });

  it("names a % that is not an escape", () => {
    expect(url("50%ZZ off")).toEqual({
      kind: "unreadable",
      reason: "“%ZZ” at character 3 is not a percent-escape: % takes two hex digits",
    });
  });

  it("reads escapes that do not spell UTF-8 as binary", () => {
    expect(decoded(url("%FF%FE%00"))).toEqual({ kind: "binary", bytes: Uint8Array.of(0xff, 0xfe, 0x00), looks: null });
  });
});

describe("jwt", () => {
  it("reads the header and payload as JSON and keeps the signature as written", () => {
    const reading = decoded(jwt(token('{"alg":"RS256"}', '{"sub":"8812"}', "AbC-_d")));
    expect(reading).toEqual({
      kind: "jwt",
      header: '{\n  "alg": "RS256"\n}',
      payload: '{\n  "sub": "8812"\n}',
      signature: "AbC-_d",
      text: '{"sub":"8812"}',
      bytes: expect.anything(),
    });
    expect(listed(reading.kind === "jwt" ? reading.bytes : null)).toEqual(listed('{"sub":"8812"}'));
  });

  it("takes padding-less segments and an empty signature", () => {
    expect(decoded(jwt(token('{"alg":"none"}', '{"a":1}', "")))).toMatchObject({ kind: "jwt", signature: "" });
  });

  it("says why when the parts, a segment's encoding or its JSON are wrong", () => {
    expect(jwt("eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiI4ODEyIn0")).toEqual({
      kind: "unreadable",
      reason: "2 parts separated by dots. A JWT has 3: header, payload, signature",
    });
    expect(jwt(`${token("{}", "{}")}.x`)).toMatchObject({ reason: "4 parts separated by dots. A JWT has 3: header, payload, signature" });
    expect(jwt(`ab*d.${b64url("{}")}.s`)).toMatchObject({
      reason: "the header is not base64url: character 3, “*”, is not in the base64 alphabet",
    });
    expect(jwt(token("{}", "plain words"))).toMatchObject({ reason: "the payload decodes, but not to JSON" });
  });
});

describe("decode", () => {
  it("guesses a JWT by its shape, URL by a %, base64 otherwise", () => {
    expect(guessDecoder("eyJhbGc.eyJz.sig")).toBe("jwt");
    expect(guessDecoder("a%20b")).toBe("url");
    expect(guessDecoder("aGVsbG8=")).toBe("base64");
  });

  it("names the decoder that reads the value, trying the guess first", () => {
    expect(decode(token('{"alg":"RS256"}', "{}"))).toMatchObject({ decoder: "jwt", reading: { kind: "jwt" } });
    expect(decode(b64("hello there"))).toMatchObject({ decoder: "base64", reading: { kind: "text", text: "hello there" } });
    expect(decode("a%20b")).toMatchObject({ decoder: "url", reading: { kind: "text", text: "a b" } });
    expect(decode("eyJub3Q6YSBqd3QifQ")).toMatchObject({ decoder: "base64" });
  });

  it("reads one way only: a %-escaped base64 value is URL, a token is never base64", () => {
    expect(decode("dGV4dA%3D%3D")).toMatchObject({ decoder: "url", reading: { text: "dGV4dA==" } });
    expect(base64(token("{}", "{}")).kind).toBe("unreadable");
  });

  it("takes URL only over a real escape", () => {
    expect(url("abcd")).toMatchObject({ kind: "text", text: "abcd" });
    expect(decode("abcd")).not.toMatchObject({ decoder: "url" });
  });

  it("reads valid base64 as binary when no format is known, as when one is", () => {
    expect(decode(b64(PROTOBUF))).toMatchObject({ decoder: "base64", reading: { kind: "binary", looks: null } });
    expect(decode(b64(GZIP))).toMatchObject({ decoder: "base64", reading: { kind: "binary", looks: "gzip" } });
  });

  it("gives the guessed decoder's reason when none reads", () => {
    expect(decode("eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiI4ODEyIn0")).toEqual({
      decoder: null,
      reason: "2 parts separated by dots. A JWT has 3: header, payload, signature",
    });
    expect(decode("50%ZZ")).toMatchObject({ reason: "“%ZZ” at character 3 is not a percent-escape: % takes two hex digits" });
  });
});

describe("time among the decoders", () => {
  const spanOf = (text: string, part: string) => ({ from: text.indexOf(part), to: text.indexOf(part) + part.length });
  const at = (text: string, part: string, into = 3) => ({ from: text.indexOf(part) + into, to: text.indexOf(part) + into });

  it("is guessed for a unit's digit count or a date-time shape, and not for other digits", () => {
    expect(guessDecoder("1793851200")).toBe("time");
    expect(guessDecoder("2026-11-05T04:00:00Z")).toBe("time");
    expect(guessDecoder("Thu, 05 Nov 2026 04:00:00 GMT")).toBe("time");
    expect(guessDecoder("17938512000")).toBe("base64");
  });

  it("reads a timestamp as time, not as the base64 its digits also are", () => {
    expect(decode("1793851200")).toMatchObject({ decoder: "time", reading: { kind: "time", utc: "2026-11-05 04:00:00Z" } });
  });

  it("reads a zone-less date-time and a date as time", () => {
    expect(decode("2026-11-05T04:00:00")).toMatchObject({ decoder: "time", reading: { readAs: "ISO 8601, no zone: read as UTC" } });
    expect(decode("2026-01-14")).toMatchObject({ decoder: "time", reading: { readAs: "ISO 8601 date", utc: null } });
  });

  it("gives time's reason for a date or date-time off the calendar, never the base64 its digits and dashes also are", () => {
    expect(decode("2026-02-30T04:00:00Z")).toEqual({ decoder: null, reason: "2026-02-30 is not a day of the calendar" });
    expect(decode("2024-02-30")).toEqual({ decoder: null, reason: "2024-02-30 is not a day of the calendar" });
    const text = '{"day": "2024-02-30"}';
    expect(decodeAround(text, { from: text.indexOf("2024") + 2, to: text.indexOf("2024") + 2 })).toMatchObject({
      peeked: { decoder: null, reason: "2024-02-30 is not a day of the calendar" },
    });
  });

  it("finds a whole date-time from a cursor or a rough selection, spaces and colons included", () => {
    const text = '{"at": "2026-11-05T04:00:00+07:00", "expires": "Thu, 05 Nov 2026 04:00:00 GMT"}';
    expect(decodeAround(text, at(text, "2026-11-05T04"))).toMatchObject({
      range: spanOf(text, "2026-11-05T04:00:00+07:00"),
      peeked: { decoder: "time", reading: { utc: "2026-11-04 21:00:00Z" } },
    });
    expect(decodeAround(text, spanOf(text, '"Thu, 05 Nov 2026 04:00:00 GMT"}'))).toMatchObject({
      range: spanOf(text, "Thu, 05 Nov 2026 04:00:00 GMT"),
      peeked: { decoder: "time", reading: { readAs: "HTTP date" } },
    });
  });

  it("finds a date whole instead of reading its year as base64", () => {
    const text = '{"homeDeploymentStartDate": "2026-01-14",';
    expect(decodeAround(text, at(text, "2026", 2))).toMatchObject({
      range: spanOf(text, "2026-01-14"),
      peeked: { decoder: "time", reading: { local: "2026-01-14, Wednesday" } },
    });
  });

  it("keeps a date-time's reason instead of reading its digits as base64", () => {
    const text = '{"at": "2026-02-30T04:00:00Z"}';
    expect(decodeAround(text, at(text, "2026"))).toMatchObject({
      range: spanOf(text, "2026-02-30T04:00:00Z"),
      peeked: { decoder: null, reason: "2026-02-30 is not a day of the calendar" },
    });
  });

  it("finds a bare timestamp the way it finds any value", () => {
    const text = '{"exp": 1793851200, "n": 1}';
    expect(decodeAround(text, at(text, "1793851200"))).toMatchObject({ range: spanOf(text, "1793851200"), peeked: { decoder: "time" } });
  });

  it("replaces with the local time and opens it as text", () => {
    const peeked = decode("1793851200");
    if (peeked.decoder === null || peeked.reading.kind !== "time") throw new Error("not read as time");
    expect(replacement(peeked.reading)).toBe(peeked.reading.local);
    expect(opensAs(peeked.reading)).toEqual({ bytes: peeked.reading.bytes, typeHint: "text/plain" });
  });
});

describe("decodeAround finds the value in a rough selection or at the cursor", () => {
  const HELLO = b64("hello world");
  const spanOf = (text: string, part: string) => ({ from: text.indexOf(part), to: text.indexOf(part) + part.length });

  it("drops the quotes and punctuation around a selected value", () => {
    const text = `{"t":"${HELLO}","u":1}`;
    const found = decodeAround(text, spanOf(text, `"${HELLO}",`));
    expect(found).toMatchObject({ range: spanOf(text, HELLO), peeked: { decoder: "base64", reading: { text: "hello world" } } });
  });

  it("grows a cursor or a selection cut short to the whole value", () => {
    const text = `{"t": "${HELLO}"}`;
    const middle = text.indexOf(HELLO) + 5;
    expect(decodeAround(text, { from: middle, to: middle })).toMatchObject({ range: spanOf(text, HELLO) });
    const cut = spanOf(text, HELLO.slice(1, -1));
    expect(decodeAround(text, cut)).toMatchObject({ range: spanOf(text, HELLO), peeked: { reading: { text: "hello world" } } });
  });

  it("keeps a selection that reads well as it is, inside a longer run", () => {
    const text = `token=${b64("hi")}&x=1`;
    expect(decodeAround(text, spanOf(text, b64("hi")))).toMatchObject({ range: spanOf(text, b64("hi")), peeked: { reading: { text: "hi" } } });
  });

  it("picks the value out of a chunk that also holds its key", () => {
    const proto = b64(PROTOBUF);
    const text = `[{"google:suggesttemplate":"${proto}"},{}]`;
    const found = decodeAround(text, spanOf(text, `{"google:suggesttemplate":"${proto}"}`));
    expect(found).toMatchObject({ range: spanOf(text, proto), peeked: { decoder: "base64", reading: { kind: "binary" } } });
  });

  it("gives the reason for the value it found when nothing reads it", () => {
    const bad = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiI4ODEyIn0";
    const text = `{"t": "${bad}"}`;
    expect(decodeAround(text, spanOf(text, bad))).toMatchObject({
      range: spanOf(text, bad),
      peeked: { decoder: null, reason: "2 parts separated by dots. A JWT has 3: header, payload, signature" },
    });
  });

  it("finds nothing where no value is", () => {
    expect(decodeAround(`{ "a": [] }`, { from: 1, to: 1 })).toBeNull();
    expect(decodeAround(`{ "a": [] }`, { from: 6, to: 9 })).toBeNull();
  });
});

describe("in JSON, a value never starts on an escape's letter", () => {
  const HELLO = b64("hello");
  const spanOf = (text: string, part: string) => ({ from: text.indexOf(part), to: text.indexOf(part) + part.length });
  const caretIn = (text: string, part: string) => ({ from: text.indexOf(part) + 3, to: text.indexOf(part) + 3 });

  it.each([
    ["a one-letter escape", `{"a":"\\n${HELLO}"}`],
    ["a backspace escape", `{"a":"\\b${HELLO}"}`],
    ["a unicode escape", `{"a":"\\u00e9${HELLO}"}`],
  ])("skips %s before the value", (_, text) => {
    expect(decodeAround(text, caretIn(text, HELLO), { json: true })).toMatchObject({
      range: spanOf(text, HELLO),
      peeked: { decoder: "base64", reading: { text: "hello" } },
    });
  });

  it("replaces into JSON that still parses, the escape kept", () => {
    const text = `{"a":"\\n${HELLO}"}`;
    const found = decodeAround(text, caretIn(text, HELLO), { json: true });
    if (found?.peeked.decoder == null) throw new Error("nothing decoded");
    const value = replacement(decoded(found.peeked.reading));
    expect(value).toBe("hello");
    expect(JSON.parse(spliceAsJson(text, found.range, value ?? ""))).toEqual({ a: "\nhello" });
  });

  it("takes the letter after an escaped backslash, which is no escape", () => {
    const text = `{"a":"\\\\${HELLO}"}`;
    expect(decodeAround(text, caretIn(text, HELLO), { json: true })).toMatchObject({ range: spanOf(text, HELLO) });
  });
});

describe("what a reading hands on", () => {
  it("replaces with the decoded text, a JWT with its payload as written, binary with nothing", () => {
    expect(replacement(decoded(url("a%20b")))).toBe("a b");
    expect(replacement(decoded(jwt(token("{}", '{"a":1}'))))).toBe('{"a":1}');
    expect(replacement(decoded(base64(b64(GZIP))))).toBeNull();
  });

  it("opens JSON and a JWT payload as JSON, text as text, binary with no hint", () => {
    const json = opensAs(decoded(base64(b64('{"a":1}'))));
    expect([listed(json.bytes), json.typeHint]).toEqual([listed('{"a":1}'), "application/json"]);
    expect(opensAs(decoded(jwt(token("{}", '{"a":1}'))))).toMatchObject({ typeHint: "application/json" });
    const text = opensAs(decoded(url("a%20b")));
    expect([listed(text.bytes), text.typeHint]).toEqual([listed("a b"), "text/plain"]);
    expect(opensAs(decoded(base64(b64(GZIP))))).toEqual({ bytes: GZIP });
  });

  it("keys the window by its bytes: equal bytes one key, different bytes another", async () => {
    const key = await contentKey(GZIP);
    expect(key).toMatch(/^decoded-[0-9a-f]{32}$/);
    expect(await contentKey(GZIP.slice())).toBe(key);
    expect(await contentKey(Uint8Array.of(1))).not.toBe(key);
  });
});
