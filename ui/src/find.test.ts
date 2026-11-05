import { describe, expect, it } from "vitest";
import { findBytes, findMatches, readHexQuery } from "./find";
import { surfaceText } from "./jsonTools";

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
