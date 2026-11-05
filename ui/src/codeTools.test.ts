import { describe, expect, it, vi } from "vitest";
import { loadBeautifier, reformatCode } from "./codeTools";

const stripped = (text: string) => text.replace(/\s+/g, "");

describe("reformatCode", () => {
  it("expands minified javascript and keeps every token", async () => {
    const beautifier = await loadBeautifier();
    const sources = [
      "function f(a){if(a){return a/2}return 1}\nlet x=1\nlet y=2",
      "const r=/ab\\/c[/]/g.test(s);const t=`line1\n${a+b} é`;",
      'var s="\\u00e9é";/* keep */var n=1;// tail\nvar m=2;',
    ];
    for (const source of sources) {
      const result = reformatCode(beautifier, "javascript", source);
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(stripped(result.text)).toBe(stripped(source));
        expect(result.text.split("\n").length).toBeGreaterThan(source.split("\n").length);
      }
    }
  });

  it("expands css with nested at-rules and braces inside strings", async () => {
    const beautifier = await loadBeautifier();
    const source = '@media (min-width:1px){a{content:"{}";color:red}b{margin:0}}';
    const result = reformatCode(beautifier, "css", source);
    expect(result).toMatchObject({ ok: true });
    if (result.ok) {
      expect(stripped(result.text)).toBe(stripped(source));
      expect(result.text).toContain("\n    color: red\n");
    }
  });

  it("keeps the text's line break and only a trailing break the source had", async () => {
    const beautifier = await loadBeautifier();
    const crlf = reformatCode(beautifier, "javascript", "a=1;\r\nb=2;\r\n", "\r\n");
    const bare = reformatCode(beautifier, "javascript", "a=1;b=2;");
    expect(crlf).toMatchObject({ ok: true });
    if (crlf.ok) {
      expect(crlf.text.replace(/\r\n/g, "")).not.toContain("\n");
      expect(crlf.text.endsWith("\r\n")).toBe(true);
    }
    expect(bare).toMatchObject({ ok: true });
    if (bare.ok) expect(bare.text.endsWith("\n")).toBe(false);
  });

  it("refuses an empty text and a result that changes more than whitespace", () => {
    const empty = reformatCode({ js: (s) => s, css: (s) => s }, "javascript", "  \n");
    expect(empty).toEqual({ ok: false, reason: "there is nothing to reformat" });
    const lossy = reformatCode({ js: () => "other", css: (s) => s }, "javascript", "a=1");
    expect(lossy).toEqual({ ok: false, reason: "the formatter would change more than whitespace" });
  });

  it("loads the formatter once", async () => {
    expect(await loadBeautifier()).toBe(await loadBeautifier());
  });

  it("a failed load is not cached: the next call tries again", async () => {
    vi.resetModules();
    vi.doMock("js-beautify", () => {
      throw new Error("chunk failed");
    });
    const fresh = await import("./codeTools");
    await expect(fresh.loadBeautifier()).rejects.toThrow();
    vi.doUnmock("js-beautify");
    vi.resetModules();
    expect(await fresh.loadBeautifier()).toHaveProperty("js");
  });
});
