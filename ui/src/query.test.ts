import { describe, expect, it } from "vitest";
import {
  chipLabel,
  chipParams,
  hasOpenQuote,
  paramOf,
  parseDraft,
  readChip,
  refusedParam,
  suggest,
  type Chip,
  type ChipKey,
} from "./query";

const chip = (key: ChipKey, value: string, negated = false): Chip => ({ key, value, negated });

describe("a draft parses to one chip", () => {
  it.each([
    ["login", chip("q", "login")],
    ['"two words"', chip("q", "two words")],
    ["-login", chip("q", "login", true)],
    ['-"two words"', chip("q", "two words", true)],
    ["host:api", chip("host", "api")],
    ["HOST:api", chip("host", "api")],
    ["host:localhost:8080", chip("host", "localhost:8080")],
    ["method:post", chip("method", "POST")],
    ["-method:OPTIONS", chip("method", "OPTIONS", true)],
    ["status:404", chip("status", "404")],
    ["status:4XX", chip("status", "4xx")],
    ["type:json", chip("type", "json")],
    ["source:Sent", chip("source", "sent")],
    ["is:pending", chip("is", "pending")],
    ["-is:https", chip("is", "https", true)],
    ["localhost:8080", chip("q", "localhost:8080")],
    ["https://api", chip("q", "https://api")],
    ["  padded  ", chip("q", "padded")],
    ["-", chip("q", "-")],
  ])("%s", (draft, expected) => {
    expect(parseDraft(draft)).toEqual({ chip: expected });
  });

  it("an empty or blank draft is nothing to make", () => {
    expect(parseDraft("")).toEqual({ empty: true });
    expect(parseDraft("   ")).toEqual({ empty: true });
  });
});

describe("a draft that does not parse says why", () => {
  it.each([
    ["stauts:4xx", "unknown key “stauts” — did you mean status?"],
    ["mehtod:GET", "unknown key “mehtod” — did you mean method?"],
    ["hots:api", "unknown key “hots” — did you mean host?"],
    ["status:", "status: needs a value — a code like 404, or a class like 4xx"],
    ["-host:", "host: needs a value — a host seen, or part of one"],
    ['"login', "a quote is opened and never closed"],
    ['"', "a quote is opened and never closed"],
    ['""', "the quotes hold nothing"],
    ["status:4", "“4” isn't a status — try a code like 404 or a class like 4xx"],
    ["status:6xx", "“6xx” isn't a status — try a code like 404 or a class like 4xx"],
    ["status:4044", "“4044” isn't a status — try a code like 404 or a class like 4xx"],
    ["method:G3T", "a method is letters only, like GET"],
    ["source:proxied", "source is captured, imported or sent"],
    ["is:done", "is is pending or https"],
  ])("%s", (draft, fault) => {
    expect(parseDraft(draft)).toEqual({ fault });
  });

  it("a one-letter key is never a near miss, so it reads as text", () => {
    expect(parseDraft("h:api")).toEqual({ chip: chip("q", "h:api") });
  });
});

describe("a chip reads back as it would be typed", () => {
  it.each([
    [chip("host", "api"), "host:api"],
    [chip("method", "GET", true), "-method:GET"],
    [chip("q", "login"), "login"],
    [chip("q", "login", true), "-login"],
    [chip("q", "two words"), '"two words"'],
    [chip("q", "status:4xx"), '"status:4xx"'],
    [chip("q", "-x"), '"-x"'],
    [chip("q", "localhost:8080"), "localhost:8080"],
  ])("%j → %s", (c, label) => {
    expect(chipLabel(c)).toBe(label);
    expect(parseDraft(label)).toEqual({ chip: c });
  });
});

describe("suggestions follow the draft", () => {
  const hosts = ["api.example.com", "auth.example.com", "localhost:8080"];
  const texts = (draft: string) => suggest(draft, hosts).map((row) => row.text);

  it("an empty draft offers the six keys", () => {
    expect(texts("")).toEqual(["host:", "method:", "status:", "type:", "source:", "is:"]);
  });

  it("a prefix offers itself as text first, then the keys it starts", () => {
    const rows = suggest("st", hosts);
    expect(rows.map((r) => [r.kind, r.text, r.hint])).toEqual([
      ["typed", "st", "search the URI for it"],
      ["key", "status:", "a code like 404, or a class like 4xx"],
    ]);
  });

  it("a value prefix offers that key's values and no as-typed row when it does not parse", () => {
    expect(texts("status:4")).toEqual(["status:4xx"]);
  });

  it("a host offers the typed text, then every host seen containing it, port kept", () => {
    expect(texts("host:api")).toEqual(["host:api", "host:api.example.com"]);
    expect(texts("host:")).toEqual(["host:api.example.com", "host:auth.example.com", "host:localhost:8080"]);
    expect(texts("host:8080")).toEqual(["host:8080", "host:localhost:8080"]);
  });

  it("a typed row equal to an offered value is not offered twice", () => {
    expect(texts("status:4xx")).toEqual(["status:4xx"]);
    expect(texts("method:get")).toEqual(["method:GET"]);
  });

  it("each key offers its own values", () => {
    expect(texts("method:")).toContain("method:OPTIONS");
    expect(texts("type:")).toEqual(
      ["json", "html", "xml", "javascript", "css", "image", "font", "form", "text"].map((t) => `type:${t}`),
    );
    expect(texts("source:")).toEqual(["source:captured", "source:imported", "source:sent"]);
    expect(texts("is:")).toEqual(["is:pending", "is:https"]);
  });

  it("a negated draft offers the same rows with -, worded as leaving out", () => {
    const rows = suggest("-me", hosts);
    expect(rows.map((r) => [r.text, r.hint])).toEqual([
      ["-me", "leave out every URI containing it"],
      ["-method:", "leave out by method"],
    ]);
    expect(texts("-status:5")).toEqual(["-status:5xx"]);
  });

  it("a fault offers nothing", () => {
    expect(texts("stauts:4xx")).toEqual([]);
  });

  it("a quoted draft offers only itself", () => {
    expect(texts('"two words"')).toEqual(['"two words"']);
  });
});

describe("the chips are sent as W1's parameters", () => {
  it("one parameter per chip, negated as not_, type as content_type, repeats kept in order", () => {
    const chips = [
      chip("status", "4xx"),
      chip("method", "GET", true),
      chip("host", "a"),
      chip("host", "b"),
      chip("type", "json"),
      chip("q", "login token"),
      chip("q", "x", true),
      chip("is", "pending"),
      chip("source", "sent", true),
    ];
    expect(chipParams(chips).toString()).toBe(
      "status=4xx&not_method=GET&host=a&host=b&content_type=json&q=login+token&not_q=x&is=pending&not_source=sent",
    );
    expect(chipParams([]).toString()).toBe("");
  });

  it("a refusal names the parameter as sent, and that names its chips", () => {
    expect(refusedParam("not_status must be a code like 404 or a class like 4xx")).toBe("not_status");
    expect(refusedParam("")).toBe("");
    expect(paramOf(chip("status", "4xx", true))).toBe("not_status");
    expect(paramOf(chip("type", "json"))).toBe("content_type");
  });
});

describe("a saved chip is read only if it parses back as itself", () => {
  it.each([
    [chip("host", "localhost:8080")],
    [chip("q", "two words", true)],
    [chip("status", "4xx")],
  ])("%j is kept", (saved) => {
    expect(readChip(saved)).toEqual(saved);
  });

  it.each([
    ["an unknown key", { key: "colour", value: "red", negated: false }],
    ["a value the grammar refuses", { key: "status", value: "6xx", negated: false }],
    ["a value the grammar would rewrite", { key: "method", value: "get", negated: false }],
    ["a missing field", { key: "q", value: "x" }],
    ["a string", "status:4xx"],
    ["null", null],
  ])("%s is dropped", (_, saved) => {
    expect(readChip(saved)).toBeNull();
  });
});

it("a quote is open while the draft holds an odd number of them", () => {
  expect(hasOpenQuote('"login')).toBe(true);
  expect(hasOpenQuote('"login token"')).toBe(false);
  expect(hasOpenQuote("login")).toBe(false);
});
