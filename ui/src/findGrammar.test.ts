import { describe, expect, it } from "vitest";
import { findLabel, keysFor, parseFind, refusedReason, suggestFind, type FindChip } from "./findGrammar";

const chip = (key: FindChip["key"], value: string): FindChip => ({ key, value });

describe("parseFind", () => {
  it("reads text, keyed chips and quotes", () => {
    expect(parseFind("")).toEqual({ empty: true });
    expect(parseFind("  ")).toEqual({ empty: true });
    expect(parseFind("login")).toEqual({ chip: chip("plain", "login") });
    expect(parseFind('"two words"')).toEqual({ chip: chip("plain", "two words") });
    expect(parseFind("key:id")).toEqual({ chip: chip("key", "id") });
    expect(parseFind("KEY:id")).toEqual({ chip: chip("key", "id") });
    expect(parseFind('value:"a b"')).toEqual({ chip: chip("value", "a b") });
    expect(parseFind("case:Token")).toEqual({ chip: chip("case", "Token") });
    expect(parseFind("re:^a+$")).toEqual({ chip: chip("re", "^a+$") });
    expect(parseFind("bytes:89 50 4e")).toEqual({ chip: chip("bytes", "89 50 4e") });
    expect(parseFind("text:PNG")).toEqual({ chip: chip("text", "PNG") });
  });

  it("an unknown key is text unless it is within two edits of a real one", () => {
    expect(parseFind("https://x.test")).toEqual({ chip: chip("plain", "https://x.test") });
    expect(parseFind("a:b")).toEqual({ chip: chip("plain", "a:b") });
    expect(parseFind("kay:id")).toEqual({ fault: "no key “kay:” — did you mean key:?" });
    expect(parseFind("rex:a")).toMatchObject({ fault: expect.stringContaining("did you mean re:") });
    expect(parseFind("regex:a")).toEqual({ chip: chip("plain", "regex:a") });
    expect(parseFind("bites:a")).toMatchObject({ fault: expect.stringContaining("did you mean bytes:") });
  });

  it("says why a draft makes no chip", () => {
    expect(parseFind("-x")).toMatchObject({ fault: expect.stringContaining("nothing to exclude") });
    expect(parseFind("-")).toEqual({ chip: chip("plain", "-") });
    expect(parseFind('"open')).toEqual({ fault: "a quote is opened and never closed" });
    expect(parseFind('" "')).toEqual({ fault: "the quotes hold nothing" });
    expect(parseFind('key:"a b')).toEqual({ fault: "a quote is opened and never closed" });
    expect(parseFind("key:")).toMatchObject({ fault: expect.stringContaining("key: needs something to find") });
    expect(parseFind("re:(")).toMatchObject({ fault: expect.stringContaining("not a regular expression") });
    expect(parseFind("bytes:8")).toEqual({ fault: "bytes: takes hex pairs, like 89 50 4e" });
    expect(parseFind("bytes:zz zz")).toEqual({ fault: "bytes: takes hex pairs, like 89 50 4e" });
  });

  it("a chip's label reads back as the same chip", () => {
    for (const made of [chip("plain", "a b"), chip("plain", "login"), chip("plain", "key:x"), chip("key", "a b"), chip("re", "x+"), chip("plain", "-1")]) {
      expect(parseFind(findLabel(made))).toEqual({ chip: made });
    }
  });
});

describe("suggestFind", () => {
  it("offers the kind's keys that the draft starts", () => {
    expect(suggestFind("", "json", [])).toEqual([]);
    expect(suggestFind("k", "json", []).map((r) => r.text)).toEqual(["key:"]);
    expect(suggestFind("c", "text", []).map((r) => r.text)).toEqual(["case:"]);
    expect(suggestFind("key", "text", [])).toEqual([]);
    expect(suggestFind("b", "hex", []).map((r) => r.text)).toEqual(["bytes:"]);
    expect(suggestFind("r", "form", []).map((r) => [r.kind, r.text])).toEqual([["key", "re:"]]);
    expect(suggestFind("x", "json", [])).toEqual([]);
    expect(suggestFind('"k', "json", [])).toEqual([]);
  });

  it("offers the document's keys after key:, quoting a name with a space", () => {
    const names = ["id", "user name", "user", "ID2"];
    expect(suggestFind("key:", "json", names).map((r) => r.text)).toEqual(["key:id", 'key:"user name"', "key:user", "key:ID2"]);
    expect(suggestFind("key:us", "json", names).map((r) => r.text)).toEqual(['key:"user name"', "key:user"]);
    expect(suggestFind("key:user", "json", names).map((r) => r.text)).toEqual(['key:"user name"']);
    expect(suggestFind("key:", "text", names)).toEqual([]);
    expect(suggestFind("value:", "json", names)).toEqual([]);
    expect(suggestFind("key:", "json", names)[0].kind).toBe("value");
  });

  it("never offers a name that holds a quote, since typing it back would read as a different name", () => {
    expect(suggestFind("key:", "json", ['a"b', "ab"]).map((r) => r.text)).toEqual(["key:ab"]);
  });

  it("shows at most a screenful of names", () => {
    const many = Array.from({ length: 50 }, (_, i) => `k${i}`);
    expect(suggestFind("key:", "json", many)).toHaveLength(12);
  });
});

describe("refusedReason", () => {
  it("refuses what a kind cannot run and names why", () => {
    expect(refusedReason(chip("key", "id"), "json")).toBeNull();
    expect(refusedReason(chip("value", "x"), "form")).toBeNull();
    expect(refusedReason(chip("key", "id"), "text")).toBe("key: needs JSON or form; this is text");
    expect(refusedReason(chip("re", "x"), "hex")).toBe("re: needs text; this is hex");
    expect(refusedReason(chip("case", "x"), "xml")).toBeNull();
    expect(refusedReason(chip("bytes", "89"), "json")).toBe("bytes: needs hex; this is json");
    expect(refusedReason(chip("text", "PNG"), "hex")).toBeNull();
    expect(refusedReason(chip("plain", "x"), "hex")).toBeNull();
  });

  it("every kind's keys are runnable on that kind", () => {
    for (const kind of ["json", "form", "xml", "html", "javascript", "css", "text", "hex"] as const) {
      for (const key of keysFor(kind)) expect(refusedReason(chip(key, "x"), kind)).toBeNull();
    }
  });
});
