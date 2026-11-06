import { describe, expect, it } from "vitest";
import { formStructure } from "./formPairs";
import { findBytes, jsonStructure, markBytes, markText, mergeRanges, readHexQuery, structureNames } from "./find";
import type { FindChip } from "./findGrammar";
import { parseJson, surfaceText } from "./jsonTools";

const findMatches = (text: string, query: string, limit?: number) => markText(text, query ? [{ key: "plain", value: query }] : [], null, limit);

describe("find", () => {
  it("matches case-insensitively, every occurrence, none for an empty query", () => {
    expect(findMatches("Login login LOGIN", "login").ranges).toEqual([
      { from: 0, to: 5 },
      { from: 6, to: 11 },
      { from: 12, to: 17 },
    ]);
    expect(findMatches("abc", "").ranges).toEqual([]);
  });

  it("reads the query literally", () => {
    expect(findMatches("a.b axb (x)", ".").ranges).toEqual([{ from: 1, to: 2 }]);
    expect(findMatches("a.b axb (x)", "(x)").ranges).toEqual([{ from: 8, to: 11 }]);
  });

  it("stops at the limit and says so", () => {
    expect(findMatches("aaaa", "a", 3)).toEqual({ ranges: [{ from: 0, to: 1 }, { from: 1, to: 2 }, { from: 2, to: 3 }], capped: true });
    expect(findMatches("aaa", "a", 3).capped).toBe(false);
  });

  it("the surface's text counts each line break as one character, whatever its kind", () => {
    const surface = surfaceText("ab\r\ncd\r\nab");
    expect(surface).toBe("ab\ncd\nab");
    expect(findMatches(surface, "ab").ranges).toEqual([
      { from: 0, to: 2 },
      { from: 6, to: 8 },
    ]);
    expect(surfaceText("a\rb\nc\r\nd")).toBe("a\nb\nc\nd");
  });
});

describe("a hex query", () => {
  it("is read as bytes when it is whole hex pairs, spaced or not", () => {
    expect(readHexQuery("de ad BE ef")).toEqual({ mode: "bytes", needle: new Uint8Array([0xde, 0xad, 0xbe, 0xef]) });
    expect(readHexQuery("cafe")).toEqual({ mode: "bytes", needle: new Uint8Array([0xca, 0xfe]) });
  });

  it("is read as UTF-8 text otherwise", () => {
    expect(readHexQuery("abc")).toEqual({ mode: "text", needle: new TextEncoder().encode("abc") });
    expect(readHexQuery("d e")).toEqual({ mode: "text", needle: new TextEncoder().encode("d e") });
    expect(readHexQuery("GET /")).toEqual({ mode: "text", needle: new TextEncoder().encode("GET /") });
    expect(Array.from(readHexQuery("é").needle)).toEqual([0xc3, 0xa9]);
  });
});

describe("find in bytes", () => {
  const rowCrossing = new Uint8Array(32).fill(0);
  rowCrossing.set([0xde, 0xad], 15);

  it("finds a byte sequence across a 16-byte row boundary", () => {
    expect(findBytes(rowCrossing, readHexQuery("dead")).ranges).toEqual([{ from: 15, to: 17 }]);
  });

  it("counts occurrences that overlap once, as text find does", () => {
    expect(findBytes(new Uint8Array([1, 1, 1, 1, 1]), readHexQuery("01 01")).ranges).toEqual([
      { from: 0, to: 2 },
      { from: 2, to: 4 },
    ]);
  });

  it("folds ASCII letters for a text query, never for bytes", () => {
    const bytes = new TextEncoder().encode("get GET Get");
    expect(findBytes(bytes, readHexQuery("GET")).ranges).toHaveLength(3);
    expect(findBytes(new Uint8Array([0x41, 0x61]), { mode: "bytes", needle: new Uint8Array([0x61]) }).ranges).toEqual([
      { from: 1, to: 2 },
    ]);
  });

  it("finds a UTF-8 query by its encoded bytes", () => {
    expect(findBytes(new TextEncoder().encode("café"), readHexQuery("é")).ranges).toEqual([{ from: 3, to: 5 }]);
  });

  it("stops at the limit and says so; an empty query finds nothing", () => {
    expect(findBytes(new Uint8Array(4), readHexQuery("00"), 3)).toEqual({
      ranges: [{ from: 0, to: 1 }, { from: 1, to: 2 }, { from: 2, to: 3 }],
      capped: true,
    });
    expect(findBytes(new Uint8Array(4), readHexQuery(""))).toEqual({ ranges: [], capped: false });
  });
});

const chip = (key: FindChip["key"], value: string): FindChip => ({ key, value });
const spans = (text: string, found: { ranges: { from: number; to: number }[] }) => found.ranges.map((r) => text.slice(r.from, r.to));

function jsonMarks(text: string, chips: FindChip[], limit?: number) {
  const parsed = parseJson(text);
  if (!parsed.ok) throw new Error(parsed.reason);
  return markText(text, chips, jsonStructure(parsed.root), limit);
}

describe("marking text with chips", () => {
  it("runs plain, re and case chips over the whole text", () => {
    const text = "Login login LOGIN";
    expect(markText(text, [chip("plain", "login")], null).ranges).toHaveLength(3);
    expect(markText(text, [chip("case", "login")], null).ranges).toEqual([{ from: 6, to: 11 }]);
    expect(spans(text, markText(text, [chip("re", "l.g")], null))).toEqual(["Log", "log", "LOG"]);
  });

  it("a regex that can match empty marks only what it consumes and ends", () => {
    const text = "aab";
    expect(spans(text, markText(text, [chip("re", "a*")], null))).toEqual(["aa"]);
    expect(markText("abc", [chip("re", "x*")], null)).toEqual({ ranges: [], capped: false });
    expect(markText("abc", [chip("re", "^")], null).ranges).toEqual([]);
  });

  it("key: marks only inside object keys and value: only inside values", () => {
    const text = '{"id": "id-7", "user": {"id": 3, "name": "ida"}, "list": ["id"]}';
    const second = text.indexOf('"id": 3') + 1;
    expect(jsonMarks(text, [chip("key", "id")]).ranges).toEqual([
      { from: 2, to: 4 },
      { from: second, to: second + 2 },
    ]);
    expect(spans(text, jsonMarks(text, [chip("value", "id")]))).toEqual(["id", "id", "id"]);
    expect(jsonMarks(text, [chip("value", "3")]).ranges).toEqual([{ from: text.indexOf("3"), to: text.indexOf("3") + 1 }]);
  });

  it("matches the decoded text of a key or value that holds escaped quotes", () => {
    const text = String.raw`{"say \"hi\"": "he said \"hi\"", "hi": 1}`;
    expect(spans(text, jsonMarks(text, [chip("key", 'say "hi"')]))).toEqual([String.raw`say \"hi\"`]);
    expect(spans(text, jsonMarks(text, [chip("value", '"hi"')]))).toEqual([String.raw`\"hi\"`]);
    expect(jsonMarks(text, [chip("key", "hi")]).ranges).toHaveLength(2);
  });

  it("a key or value spelled with escapes is found by its plain spelling, and the other way", () => {
    const text = String.raw`{"caf\u00e9": "a\/b", "é": "\u00C9"}`;
    expect(jsonMarks(text, [chip("key", "café")]).ranges).toHaveLength(1);
    expect(jsonMarks(text, [chip("key", "é")]).ranges).toHaveLength(2);
    expect(jsonMarks(text, [chip("value", "a/b")]).ranges).toHaveLength(1);
    expect(jsonMarks(text, [chip("value", "É")]).ranges).toHaveLength(1);
    expect(jsonMarks(text, [chip("case", "é")]).ranges.length).toBeGreaterThan(0);
  });

  it("a character outside the BMP is found whole, escaped as a pair, raw, or mixed, and followed by more text", () => {
    const text = String.raw`{"a": "\ud83d\ude00x", "b": "😀x", "c": "😀\ude00"}`;
    expect(jsonMarks(text, [chip("value", "😀x")]).ranges).toHaveLength(2);
    expect(jsonMarks(text, [chip("value", "😀")]).ranges).toHaveLength(3);
  });

  it("key: and value: over text that is not a structure mark nothing", () => {
    expect(markText('{"a":1}', [chip("key", "a")], null).ranges).toEqual([]);
  });

  it("form text: key: and value: use the pair's raw parts", () => {
    const text = "scope=read&scope=write&id=scope";
    const structure = formStructure(text);
    expect(markText(text, [chip("key", "scope")], structure).ranges).toEqual([
      { from: 0, to: 5 },
      { from: 11, to: 16 },
    ]);
    expect(markText(text, [chip("value", "scope")], structure).ranges).toEqual([{ from: 26, to: 31 }]);
    expect(formStructure("").keys).toEqual([]);
    expect(formStructure("flag").values).toEqual([{ from: 4, to: 4 }]);
  });

  it("a URL's key: and value: mark only its query, never the head or the fragment", () => {
    const text = "https://a.example/a=b?a=1&b=a#a=z";
    const structure = formStructure(text, "url");
    expect(markText(text, [chip("key", "a")], structure).ranges).toEqual([{ from: 22, to: 23 }]);
    expect(markText(text, [chip("value", "a")], structure).ranges).toEqual([{ from: 28, to: 29 }]);
    expect(formStructure("https://a.example/#f", "url")).toEqual({ keys: [], values: [], jsonEscaped: false, percentEncoded: true });
  });

  it("header lines: key: marks the name, value: the value after the colon and its spaces", () => {
    const text = "accept: a\r\nx-a:  b\r\n";
    const structure = formStructure(text, "headers");
    expect(structure.keys).toEqual([
      { from: 0, to: 6 },
      { from: 11, to: 14 },
    ]);
    expect(structure.values).toEqual([
      { from: 8, to: 9 },
      { from: 17, to: 18 },
    ]);
    expect(markText(text, [chip("value", "a")], structure).ranges).toEqual([{ from: 8, to: 9 }]);
    expect(formStructure('{"a":1}', "none").keys).toEqual([]);
  });

  it("overlapping chips mark once and the union is in document order", () => {
    const text = "the user name and user";
    const found = markText(text, [chip("plain", "user name"), chip("plain", "user"), chip("plain", "name")], null);
    expect(spans(text, found)).toEqual(["user name", "user"]);
    expect(markText(text, [chip("plain", "user"), chip("plain", "user")], null).ranges).toHaveLength(2);
    expect(found.ranges.map((r) => r.from)).toEqual([4, 18]);
  });

  it("stops at the limit across every chip and says so", () => {
    const text = "ab ".repeat(10);
    const found = markText(text, [chip("plain", "a"), chip("plain", "b")], null, 5);
    expect(found.ranges).toHaveLength(5);
    expect(found.capped).toBe(true);
    expect(markText("a b", [chip("plain", "a"), chip("plain", "b")], null, 2).capped).toBe(false);
  });

  it("says capped when more matches follow a count that lands exactly on the limit", () => {
    const text = '{"a": "x", "b": "x", "c": "x"}';
    expect(jsonMarks(text, [chip("value", "x")], 3).capped).toBe(false);
    expect(jsonMarks(text, [chip("value", "x")], 2)).toMatchObject({ capped: true });
    const split = jsonMarks('{"a": "x", "b": "x", "c": "y"}', [chip("value", "x")], 2);
    expect(split.capped).toBe(false);
    expect(jsonMarks(text, [chip("value", "x")], 2).ranges).toHaveLength(2);
  });

  it("one chip scans many parts and keeps no state between them", () => {
    const text = JSON.stringify(Object.fromEntries(Array.from({ length: 50 }, (_, i) => [`k${i}`, "aa"])));
    expect(jsonMarks(text, [chip("value", "a")]).ranges).toHaveLength(100);
  });

  it("merges ranges that overlap but not ones that only touch", () => {
    expect(
      mergeRanges([
        { ranges: [{ from: 0, to: 4 }, { from: 10, to: 12 }], capped: false },
        { ranges: [{ from: 2, to: 6 }, { from: 6, to: 8 }], capped: false },
      ]).ranges,
    ).toEqual([
      { from: 0, to: 6 },
      { from: 6, to: 8 },
      { from: 10, to: 12 },
    ]);
  });
});

describe("a structure's key names", () => {
  it("lists each distinct key once, decoded", () => {
    const text = String.raw`{"a": 1, "b": {"a": 2, "x\ny": 3}}`;
    const parsed = parseJson(text);
    if (!parsed.ok) throw new Error("parse");
    expect(structureNames(jsonStructure(parsed.root), text, true)).toEqual(["a", "b", "x\ny"]);
    expect(structureNames(formStructure("a%20b=1&a%20b=2&c=3"), "a%20b=1&a%20b=2&c=3", true)).toEqual(["a b", "c"]);
    expect(structureNames(formStructure("a%20b=1&c=3"), "a%20b=1&c=3", false)).toEqual(["a%20b", "c"]);
    expect(structureNames(formStructure("a=1&b=2&c=3"), "a=1&b=2&c=3", true, 2)).toEqual(["a", "b"]);
    expect(structureNames(formStructure("a%20b: 1", "headers"), "a%20b: 1", true)).toEqual(["a%20b"]);
    expect(structureNames(formStructure("http://h/?a%20b=1", "url"), "http://h/?a%20b=1", true)).toEqual(["a b"]);
  });
});

describe("marking bytes with chips", () => {
  const bytes = new TextEncoder().encode("GET /a HTTP/1.1\r\nget /b");

  it("bytes: is exact hex, text: is ASCII folded, plain keeps the guess", () => {
    expect(markBytes(bytes, [chip("bytes", "47 45 54")]).ranges).toEqual([{ from: 0, to: 3 }]);
    expect(markBytes(bytes, [chip("text", "get")]).ranges).toHaveLength(2);
    const plain = markBytes(bytes, [chip("plain", "get")]);
    expect(plain.ranges).toHaveLength(2);
    expect(plain.as).toBe("text");
    expect(markBytes(new Uint8Array([0xde, 0xad]), [chip("plain", "de ad")]).as).toBe("bytes");
    expect(markBytes(bytes, [chip("bytes", "47 45 54")]).as).toBeNull();
  });

  it("chips a hex view cannot run mark nothing; the union merges and caps", () => {
    expect(markBytes(bytes, [chip("key", "GET"), chip("re", "GET")]).ranges).toEqual([]);
    expect(markBytes(bytes, [chip("bytes", "47 45"), chip("text", "get")]).ranges).toHaveLength(2);
    expect(markBytes(new Uint8Array(10), [chip("bytes", "00"), chip("bytes", "00 00")], 4)).toMatchObject({ capped: true });
  });
});
