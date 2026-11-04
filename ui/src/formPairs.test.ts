import { describe, expect, it } from "vitest";
import { addPairAfter, editPair, findInPairs, pairsAsJson, parsePairs, removePair, repeats, sortByKey } from "./formPairs";

const FORM = "grant_type=urn%3Aietf&scope=accounts&scope=sessions%20devices&state=&note=50%ZZ+off&flag";

describe("pairs are read for display, in wire order", () => {
  it("decodes each key and value, + as a space", () => {
    const pairs = parsePairs(FORM);
    expect(pairs.map((p) => [p.key, p.value])).toEqual([
      ["grant_type", "urn:ietf"],
      ["scope", "accounts"],
      ["scope", "sessions devices"],
      ["state", ""],
      ["note", "50%ZZ+off"],
      ["flag", ""],
    ]);
  });

  it("a bad escape is kept as sent and says so", () => {
    const note = parsePairs(FORM)[4];
    expect(note.kept).toBe(true);
    expect(note.raw).toBe("note=50%ZZ+off");
    expect(parsePairs(FORM).filter((p) => p.kept)).toHaveLength(1);
  });

  it("an empty form has no pairs", () => {
    expect(parsePairs("")).toEqual([]);
  });

  it("a repeated key reads which of how many it is", () => {
    const counted = repeats(parsePairs(FORM));
    expect([...counted]).toEqual([
      [1, { nth: 1, of: 2 }],
      [2, { nth: 2, of: 2 }],
    ]);
  });
});

describe("an edit re-encodes only its own pair", () => {
  it("every other pair keeps its exact sent bytes", () => {
    const edited = editPair(FORM, 2, "value", "a b&c");
    expect(edited).toBe(
      "grant_type=urn%3Aietf&scope=accounts&scope=a%20b%26c&state=&note=50%ZZ+off&flag",
    );
    expect(parsePairs(edited)[2].value).toBe("a b&c");
  });

  it("a key edit keeps the pair's value", () => {
    expect(editPair("a=1&b=2", 0, "key", "z z")).toBe("z%20z=1&b=2");
  });

  it("the edited part is encoded; the part nobody edited keeps its sent bytes", () => {
    expect(editPair("note=50%ZZ", 0, "value", "50%")).toBe("note=50%25");
    expect(editPair("%ZZ=ok", 0, "value", "new")).toBe("%ZZ=new");
    expect(editPair("a+b=1%7e", 0, "key", "c d")).toBe("c%20d=1%7e");
    expect(editPair("flag", 0, "value", "on")).toBe("flag=on");
    expect(editPair("flag", 0, "key", "on air")).toBe("on%20air");
  });

  it("a lone surrogate in an edit is written as the replacement character, not thrown", () => {
    expect(editPair("a=1", 0, "value", "x\ud800")).toBe("a=x%EF%BF%BD");
    expect(editPair("a=1", 0, "key", "\udc00")).toBe("%EF%BF%BD=1");
    expect(parsePairs(editPair("%ZZ=ok", 0, "value", "new"))[0].kept).toBe(true);
  });
});

describe("pairs are added and removed in place", () => {
  it("adds an empty pair after the one named, or first after -1", () => {
    expect(addPairAfter("a=1&b=2", 0)).toBe("a=1&=&b=2");
    expect(addPairAfter("a=1", 0)).toBe("a=1&=");
    expect(addPairAfter("", -1)).toBe("=");
    expect(parsePairs(addPairAfter("a=1", 0))[1]).toMatchObject({ key: "", value: "" });
  });

  it("removes exactly the pair named, duplicates and all", () => {
    expect(removePair("a=1&a=2&b=3", 1)).toBe("a=1&b=3");
    expect(removePair("a=1", 0)).toBe("");
  });
});

describe("sort by key", () => {
  it("orders pairs by their decoded key, stable among repeats, each pair's bytes as written", () => {
    expect(sortByKey("b=2&a=1&b=&a=%ZZ")).toBe("a=1&a=%ZZ&b=2&b=");
    expect(sortByKey("z%20y=1&z+a=2&=3")).toBe("=3&z+a=2&z%20y=1");
  });

  it("leaves an empty form empty", () => {
    expect(sortByKey("")).toBe("");
  });
});

describe("the pairs as JSON", () => {
  it("keeps the first-seen key order; a repeated key holds every value in wire order", () => {
    expect(pairsAsJson(parsePairs("b=2&a=1&b=&a=%ZZ"))).toBe('{\n  "b": [\n    "2",\n    ""\n  ],\n  "a": [\n    "1",\n    "%ZZ"\n  ]\n}');
  });

  it("keeps an integer-like key in its place and __proto__ as a key", () => {
    const json = pairsAsJson(parsePairs("z=1&2=two&__proto__=p"));
    expect(json).toBe('{\n  "z": "1",\n  "2": "two",\n  "__proto__": "p"\n}');
    expect(JSON.parse(json)).toEqual(JSON.parse('{"z": "1", "2": "two", "__proto__": "p"}'));
  });

  it("writes a lone surrogate as an escape, so the JSON stays valid", () => {
    expect(pairsAsJson([{ index: 0, raw: "", key: "k", value: "\ud800", kept: false }])).toBe('{\n  "k": "\\ud800"\n}');
  });

  it("is an empty object for no pairs", () => {
    expect(pairsAsJson([])).toBe("{}");
  });
});

describe("find in pairs", () => {
  const pairs = parsePairs("scope=accounts&Scope=x&note=none&s=scope%20scope");

  it("counts over keys then values, pair by pair, case-insensitive, numbering each pair's first", () => {
    const found = findInPairs(pairs, "scope");
    expect(found.total).toBe(4);
    expect(found.capped).toBe(false);
    expect([...found.byPair.keys()]).toEqual([0, 1, 3]);
    expect(found.byPair.get(3)).toEqual({ first: 2, key: [], value: [{ from: 0, to: 5 }, { from: 6, to: 11 }] });
  });

  it("stops at the limit and says so; an empty query finds nothing", () => {
    expect(findInPairs(pairs, "scope", 3)).toMatchObject({ total: 3, capped: true });
    expect(findInPairs(pairs, "")).toEqual({ byPair: new Map(), total: 0, capped: false });
  });
});
