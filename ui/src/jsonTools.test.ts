import { describe, expect, it } from "vitest";
import {
  INDENT_UNIT,
  indentChosen,
  indentOf,
  layoutJson,
  nodeAt,
  parseJson,
  pathText,
  reformatJson,
  spliceAsJson,
  valueText,
  type JsonNode,
} from "./jsonTools";

function rootOf(text: string): JsonNode {
  const parsed = parseJson(text);
  if (!parsed.ok) throw new Error(`fixture does not parse: ${parsed.reason}`);
  return parsed.root;
}

function reformatted(text: string, indent: "2" | "4" | "tab" | "min", sort = false, lineBreak = "\n"): string {
  const result = reformatJson(text, INDENT_UNIT[indent], sort, lineBreak);
  if (!result.ok) throw new Error(`fixture does not parse: ${result.reason}`);
  return result.text;
}

describe("parse", () => {
  it("accepts what JSON.parse accepts, at any value at the root", () => {
    for (const text of ['{"a":[1,-2.5e+3,true,false,null,"x\\u00e9\\n"]}', "[]", "{}", '"s"', "0", " null "]) {
      expect(parseJson(text).ok).toBe(true);
    }
  });

  it("keeps each node's range in the text", () => {
    const text = '{"a": [10, "x"]}';
    const root = rootOf(text);
    expect(root).toMatchObject({ type: "object", from: 0, to: text.length });
    if (root.type !== "object") throw new Error("object expected");
    const array = root.members[0].node;
    expect(text.slice(array.from, array.to)).toBe('[10, "x"]');
    expect(root.members[0]).toMatchObject({ key: "a", keyRaw: '"a"', from: 1 });
  });

  it.each([
    ["", 1, 1, "the text ends before the JSON does"],
    ['{"a": 1,}', 1, 9, "expected a key in double quotes"],
    ["[1, 2,]", 1, 7, "expected a value: an object, array, string, number, true, false or null"],
    ['{"a" 1}', 1, 6, "expected : after the key"],
    ["{\n  \"a\": 1\n  \"b\": 2\n}", 3, 3, "expected , or }"],
    ["{\r\n\"a\": tru\r\n}", 2, 6, "expected a value: an object, array, string, number, true, false or null"],
    ['["a\nb"]', 1, 4, "a control character or line break inside a string"],
    ['["\\x"]', 1, 3, "\\x is not an escape JSON knows"],
    ['["\\u12"]', 1, 3, "\\u needs four hex digits"],
    ['{"a": "open', 1, 7, "a string is not closed"],
    ["[1] [2]", 1, 5, "there is more text after the JSON ends"],
    ["[01]", 1, 3, "expected , or ]"],
    ["[-]", 1, 2, "a number is not written the way JSON writes numbers"],
  ])("%j does not parse: line %i, column %i, %s", (text, line, column, reason) => {
    expect(parseJson(text)).toEqual({ ok: false, line, column, reason });
  });

  it("reads past a leading byte-order mark and says it was there", () => {
    expect(parseJson('\uFEFF{"a":1}')).toMatchObject({ ok: true, bom: true });
    expect(parseJson('{"a":1}')).toMatchObject({ ok: true, bom: false });
  });

  it("parses nesting far deeper than a call stack holds", () => {
    const depth = 100_000;
    expect(parseJson("[".repeat(depth) + "]".repeat(depth)).ok).toBe(true);
  });
});

describe("reformat", () => {
  const doc = '{"b":{"y":[1,{"z":true}],"x":null},"a":[]}';

  it("lays out at 2, 4, a tab, or minified, as JSON.stringify does", () => {
    const value = JSON.parse(doc) as unknown;
    expect(reformatted(doc, "2")).toBe(JSON.stringify(value, null, 2));
    expect(reformatted(doc, "4")).toBe(JSON.stringify(value, null, 4));
    expect(reformatted(doc, "tab")).toBe(JSON.stringify(value, null, "\t"));
    expect(reformatted(JSON.stringify(value, null, 2), "min")).toBe(doc);
  });

  it("keeps numbers past 2^53 as written", () => {
    expect(reformatted('{ "id": 9007199254740993, "big": 1.000000000000000000001e400 }', "min")).toBe(
      '{"id":9007199254740993,"big":1.000000000000000000001e400}',
    );
  });

  it("keeps every member of a repeated key, in order", () => {
    expect(reformatted('{"a": 1, "b": 0, "a": 2}', "min")).toBe('{"a":1,"b":0,"a":2}');
  });

  it("keeps string escapes as written", () => {
    expect(reformatted('["\\u00e9", "\\/", "é", "\\ud800"]', "min")).toBe('["\\u00e9","\\/","é","\\ud800"]');
  });

  it("keeps a byte-order mark and a closing line break", () => {
    expect(reformatted('\uFEFF{"a":1}\n', "2")).toBe('\uFEFF{\n  "a": 1\n}\n');
    expect(reformatted('{"a":1}', "2")).toBe('{\n  "a": 1\n}');
  });

  it("writes the text's own line break", () => {
    expect(reformatted('{"a":[1]}\r\n', "2", false, "\r\n")).toBe('{\r\n  "a": [\r\n    1\r\n  ]\r\n}\r\n');
  });

  it("lays out nesting far deeper than a call stack holds", () => {
    const depth = 50_000;
    const deep = "[".repeat(depth) + "]".repeat(depth);
    expect(reformatted(`[${deep}]`, "min")).toBe(`[${deep}]`);
    const shallow = 1_000;
    const indented = reformatted("[".repeat(shallow) + "1" + "]".repeat(shallow), "2");
    expect(indented).toBe(JSON.stringify(JSON.parse("[".repeat(shallow) + "1" + "]".repeat(shallow)), null, 2));
  });

  it("sorts keys at every depth, leaving array order alone", () => {
    expect(reformatted(doc, "min", true)).toBe('{"a":[],"b":{"x":null,"y":[1,{"z":true}]}}');
  });

  it("sorts by the key's value, not its escapes, keeping repeated keys in their order", () => {
    expect(reformatted('{"b":1,"\\u0061":2,"a":3,"b":4}', "min", true)).toBe('{"\\u0061":2,"a":3,"b":1,"b":4}');
  });

  it.each([
    ["8 spaces", "        "],
    ["3 spaces", "   "],
    ["a space then a tab", " \t"],
    ["no indent, one member a line", ""],
  ])("sorting keeps a text written with %s exactly as indented", (_, unit) => {
    const text = `{\n${unit}"b": [\n${unit}${unit}1\n${unit}],\n${unit}"a": 2\n}\n`;
    const sorted = reformatJson(text, indentOf(text), true, "\n");
    expect(sorted).toEqual({ ok: true, text: `{\n${unit}"a": 2,\n${unit}"b": [\n${unit}${unit}1\n${unit}]\n}\n` });
  });

  it("says where a text that does not parse fails, and changes nothing", () => {
    expect(reformatJson('{"a": }', "  ", false, "\n")).toMatchObject({ ok: false, line: 1, column: 7 });
  });

  it("layoutJson writes a lone scalar as it was written", () => {
    expect(layoutJson(rootOf("1E400"), "  ", true, "\n")).toBe("1E400");
  });
});

describe("the indent a text is written with", () => {
  it.each([
    ['{"a":1}', null],
    ['{\n  "a": 1\n}', "  "],
    ['{\n        "a": 1\n}', "        "],
    ['{\r\n\t"a": 1\r\n}', "\t"],
    ['\uFEFF{\n"a": 1\n}\n', ""],
  ])("%j is indented by %j", (text, unit) => {
    expect(indentOf(text)).toBe(unit);
  });

  it("a reformat step names its choice; any other step none", () => {
    expect(indentChosen("indent 4 spaces")).toBe("4");
    expect(indentChosen("minified")).toBe("min");
    expect(indentChosen("typed on line 2")).toBeNull();
  });
});

describe("the node at an offset", () => {
  const text = '{"items": [{"name": "a b"}, 42], "odd key": {"x": 1}}';
  const root = rootOf(text);
  const at = (needle: string, shift = 0) => nodeAt(root, text.indexOf(needle) + shift);

  it("inside a nested value: its path and value", () => {
    const found = at('"a b"', 2);
    expect(pathText(found.path)).toBe("$.items[0].name");
    expect(valueText(text, found.node)).toBe("a b");
  });

  it("on a key: the member it names", () => {
    const found = at('"name"', 1);
    expect(pathText(found.path)).toBe("$.items[0].name");
  });

  it("an array item, and a value at its very end", () => {
    expect(pathText(at("42").path)).toBe("$.items[1]");
    expect(pathText(at("42", 2).path)).toBe("$.items[1]");
    expect(valueText(text, at("42").node)).toBe("42");
  });

  it("a key that is no identifier is quoted in brackets", () => {
    expect(pathText(at('"x"').path)).toBe('$["odd key"].x');
  });

  it("between members: the container, its value verbatim", () => {
    const found = at(", 42", 1);
    expect(pathText(found.path)).toBe("$.items");
    expect(valueText(text, found.node)).toBe('[{"name": "a b"}, 42]');
  });

  it("outside every member: the root", () => {
    expect(pathText(nodeAt(root, 0).path)).toBe("$");
  });

  it("a container's value is written with the text's line break", () => {
    const lined = '{"a": [\n  1\n]}';
    expect(valueText(lined, nodeAt(rootOf(lined), 7).node, "\r\n")).toBe("[\r\n  1\r\n]");
  });

  it("a string's escapes are decoded in its value", () => {
    const escaped = '{"a": "\\u00e9\\n"}';
    expect(valueText(escaped, nodeAt(rootOf(escaped), 8).node)).toBe("é\n");
  });
});

describe("spliceAsJson", () => {
  const spanOf = (text: string, part: string) => ({ from: text.indexOf(part), to: text.indexOf(part) + part.length });

  it("writes a value inside a string as its escaped content", () => {
    const text = '{"t": "a b c"}';
    expect(spliceAsJson(text, spanOf(text, "b"), 'say "hi"')).toBe('{"t": "a say \\"hi\\" c"}');
  });

  it("writes a value over a bare number or literal as a quoted string", () => {
    const text = '{"exp": 1793851200, "ok": true}';
    expect(spliceAsJson(text, spanOf(text, "1793851200"), "2026-11-05 11:00:00 +07:00")).toBe(
      '{"exp": "2026-11-05 11:00:00 +07:00", "ok": true}',
    );
    expect(spliceAsJson(text, spanOf(text, "true"), "yes")).toBe('{"exp": 1793851200, "ok": "yes"}');
  });

  it("writes escaped content over part of a number, a key, or a working copy that does not parse", () => {
    const text = '{"exp": 1793851200}';
    expect(spliceAsJson(text, spanOf(text, "17938"), "x")).toBe('{"exp": x51200}');
    expect(spliceAsJson(text, spanOf(text, "exp"), 'a"b')).toBe('{"a\\"b": 1793851200}');
    const broken = '{"exp": 1793851200,';
    expect(spliceAsJson(broken, spanOf(broken, "1793851200"), "x")).toBe('{"exp": x,');
  });
});
