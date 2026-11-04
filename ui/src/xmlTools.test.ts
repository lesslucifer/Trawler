import { describe, expect, it } from "vitest";
import { reindentXml } from "./xmlTools";

function reindented(text: string, lineBreak = "\n"): string {
  const result = reindentXml(text, lineBreak);
  if (!result.ok) throw new Error(`fixture is not well-formed: ${result.reason}`);
  return result.text;
}

const lines = (...rows: string[]) => rows.join("\n");

describe("re-indent", () => {
  it("puts a tag a line, nested by two spaces when the text names no unit", () => {
    expect(reindented('<?xml version="1.0"?><a><b x="1"><c/></b><d></d></a>')).toBe(
      lines('<?xml version="1.0"?>', "<a>", '  <b x="1">', "    <c/>", "  </b>", "  <d></d>", "</a>"),
    );
  });

  it("keeps the unit the text is already indented with, and is stable on its own output", () => {
    const tabbed = "<a>\n\t<b>\n<c/></b>\n</a>";
    expect(reindented(tabbed)).toBe("<a>\n\t<b>\n\t\t<c/>\n\t</b>\n</a>");
    expect(reindented(reindented(tabbed))).toBe(reindented(tabbed));
  });

  it("keeps an element holding text as written, mixed content included", () => {
    expect(reindented("<a><b>two  words</b><p>Hi <i>there</i>  you</p></a>")).toBe(
      lines("<a>", "  <b>two  words</b>", "  <p>Hi <i>there</i>  you</p>", "</a>"),
    );
  });

  it("keeps a comment, CDATA, a doctype with an internal subset and an attribute holding >", () => {
    expect(reindented('<!DOCTYPE a [<!ENTITY e "x">]><a q="1>0"><!-- a\n note --><b><![CDATA[<raw>]]></b></a>')).toBe(
      lines('<!DOCTYPE a [<!ENTITY e "x">]>', '<a q="1>0">', "  <!-- a\n note -->", "  <b><![CDATA[<raw>]]></b>", "</a>"),
    );
  });

  it("reads HTML: void elements stand alone, script text is kept raw, tag case is matched loosely", () => {
    expect(reindented('<HTML><head><meta charset="utf-8"><script>if (a < b) x()</script></head><body><br></BODY></html>')).toBe(
      lines("<HTML>", "  <head>", '    <meta charset="utf-8">', "    <script>if (a < b) x()</script>", "  </head>", "  <body>", "    <br>", "  </BODY>", "</html>"),
    );
  });

  it("ends raw script text where it closes, whatever case-folding does to the text before it", () => {
    expect(reindented("<a><b>İİ</b><script>İİİİ x</script><c/></a>")).toBe(
      lines("<a>", "  <b>İİ</b>", "  <script>İİİİ x</script>", "  <c/>", "</a>"),
    );
  });

  it("reads a void name that is closed as an element, so RSS stays XML", () => {
    expect(reindented("<channel><link>http://x</link><title>t</title><source url='u'/></channel>")).toBe(
      lines("<channel>", "  <link>http://x</link>", "  <title>t</title>", "  <source url='u'/>", "</channel>"),
    );
  });

  it("an unclosed void element holds nothing: its text and elements belong to its parent", () => {
    expect(reindented("<div><p>one<br>two</p><input><b>x</b></div>")).toBe(
      lines("<div>", "  <p>one<br>two</p>", "  <input>", "  <b>x</b>", "</div>"),
    );
  });

  it("nests 10 000 deep without recursion", () => {
    const deep = "<a>".repeat(10_000) + "</a>".repeat(10_000);
    const out = reindented(deep).split("\n");
    expect(out).toHaveLength(19_999);
    expect(out[9_999]).toBe(`${"  ".repeat(9_999)}<a></a>`);
  });

  it("writes the text's own line break and keeps a BOM and a closing break", () => {
    expect(reindented("﻿<a><b/></a>\r\n", "\r\n")).toBe("﻿<a>\r\n  <b/>\r\n</a>\r\n");
  });

  it("leaves a document with text outside its elements as written", () => {
    expect(reindented("hello <b>x</b><i>y</i>")).toBe("hello <b>x</b><i>y</i>");
  });
});

describe("a text that is not well-formed", () => {
  it.each([
    ["<a><b></a>", 1, 7, "</a> closes <b>"],
    ["<a>\n</b>", 2, 1, "</b> closes <a>"],
    ["</a>", 1, 1, "</a> closes nothing"],
    ["<a>\r\n  <b>", 2, 3, "<b> is never closed"],
    ["<a><!-- open", 1, 4, "a comment that never ends"],
    ["<a><![CDATA[x", 1, 4, "a CDATA section that never ends"],
    ['<a x="1>', 1, 1, "a tag that never ends"],
    ["<a>1 < 2</a>", 1, 6, "a < that opens no tag"],
    ["<a><br><b></a>", 1, 11, "</a> closes <b>"],
  ])("%j says line %i, column %i: %s", (text, line, column, reason) => {
    expect(reindentXml(text)).toEqual({ ok: false, line, column, reason });
  });
});
