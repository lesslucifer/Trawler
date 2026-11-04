import { describe, expect, it } from "vitest";
import {
  base64,
  contentKey,
  decode,
  guessDecoder,
  jwt,
  opensAs,
  replacement,
  url,
  type Decoded,
  type Reading,
} from "./decoders";

const b64 = (bytes: Uint8Array | string) =>
  btoa(String.fromCharCode(...(typeof bytes === "string" ? new TextEncoder().encode(bytes) : bytes)));
const b64url = (text: string) => b64(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const token = (header: string, payload: string, signature = "c2ln") => `${b64url(header)}.${b64url(payload)}.${signature}`;
const GZIP = Uint8Array.of(0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03);

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

  it("takes URL only over a real escape and base64-to-binary only with a known signature", () => {
    expect(url("abcd")).toMatchObject({ kind: "text", text: "abcd" });
    expect(decode("abcd")).toEqual({ decoder: null, reason: "it decodes to nothing readable" });
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
