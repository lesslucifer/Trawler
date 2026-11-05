import { describe, expect, it, vi } from "vitest";
import { previewDocument } from "./htmlPreview";

const parsed = (source: string) => new DOMParser().parseFromString(previewDocument(source), "text/html");
const POLICY = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; form-action 'none'";

describe("previewDocument", () => {
  it("puts the policy first in head, before every author node", () => {
    const page = parsed(
      '<!-- lead --><!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src *"><title>t</title><style>p{color:red}</style></head><body><img src="https://x.test/a.png"></body></html>',
    );
    const first = page.head.firstElementChild!;
    expect(first.getAttribute("http-equiv")).toBe("Content-Security-Policy");
    expect(first.getAttribute("content")).toBe(POLICY);
    expect(page.head.querySelectorAll("meta[http-equiv]")).toHaveLength(1);
    expect(page.querySelector("title")?.textContent).toBe("t");
    expect(page.querySelector("style")).not.toBeNull();
  });

  it("puts the light page default after the policy and ahead of the page's own styles", () => {
    const page = parsed("<style>p{color:red}</style><p>x</p>");
    const styles = [...page.head.querySelectorAll("style")].map((node) => node.textContent);
    expect(styles[0]).toContain("color-scheme:light");
    expect(styles[1]).toBe("p{color:red}");
  });

  it.each([
    ['<meta http-equiv="refresh" content="0;url=https://x.test">'],
    ['<META HTTP-EQUIV=REFRESH CONTENT="0;url=//x.test">'],
    ['<meta content="0;url=https://x.test"\n  http-equiv = "Refresh">'],
    ['<meta/http-equiv=refresh content="1;url=x">'],
    ['<meta http-equiv="re&#102;resh" content="0;url=https://x.test">'],
  ])("removes a meta refresh: %s", (tag) => {
    const page = parsed(`<html><head>${tag}</head><body>x</body></html>`);
    expect([...page.querySelectorAll("meta")].filter((meta) => /refresh/i.test(meta.outerHTML))).toHaveLength(0);
    expect(previewDocument(`<p>${tag}</p>`).toLowerCase()).not.toContain("url=");
  });

  it("removes base under any case, in head or body, and keeps charset meta", () => {
    const page = parsed('<head><BASE href="https://x.test/"><meta charset="utf-8"></head><body><base href="//y.test/">x</body>');
    expect(page.querySelectorAll("base[href]")).toHaveLength(0);
    expect(page.querySelector("meta[charset]")).not.toBeNull();
  });

  it("targets every link away so a click goes nowhere", () => {
    const page = parsed('<a href="https://x.test" target="_self">a</a><area href="/y"><a name="n">b</a>');
    expect(page.querySelector("a[href]")?.getAttribute("target")).toBe("_blank");
    expect(page.querySelector("area")?.getAttribute("target")).toBe("_blank");
    expect(page.querySelector("a[name]")?.getAttribute("target")).toBe("_blank");
  });

  it("targets svg links away too, whichever attribute carries the address", () => {
    const page = parsed('<svg xmlns:xlink="http://www.w3.org/1999/xlink"><a xlink:href="https://x.test"><text>go</text></a><a href="https://y.test"><text>go</text></a></svg>');
    const links = [...page.querySelectorAll("svg a")];
    expect(links).toHaveLength(2);
    expect(links.every((link) => link.getAttribute("target") === "_blank")).toBe(true);
  });

  it("drops template, whose declarative shadow root the walk cannot reach, and link hints", () => {
    const out = previewDocument(
      '<div><template shadowrootmode="open"><a href="https://x.test">go</a><base href="//y.test"></template></div><link rel="preconnect" href="https://x.test"><link rel="stylesheet" href="https://x.test/a.css">',
    );
    expect(out).not.toContain("template");
    expect(out).not.toContain("x.test");
    expect(out).not.toContain("y.test");
  });

  it("drops nested frames and plugins, whose srcdoc the walk never enters, and a link's ping and download", () => {
    const out = previewDocument(
      '<iframe srcdoc="&lt;meta http-equiv=refresh content=0;url=https://x.test&gt;"></iframe><object data="https://x.test"></object><embed src="https://x.test"><a href="/y" ping="https://x.test/p" download="f">go</a>',
    );
    expect(out).not.toMatch(/iframe|object|embed|x\.test/);
    expect(out).not.toMatch(/ping|download/);
  });

  it("what the frame reparses is what was cleaned: no second policy, base or refresh appears", () => {
    const payloads = [
      '<svg><style><a title="</style><meta http-equiv=refresh content=0;url=//x.test>"></a></style></svg>',
      '<math><mtext><table><mglyph><style><!--</style><meta http-equiv=refresh content="0;url=//x.test">--></style></mglyph></table></mtext></math>',
      '<noscript><p title="</noscript><meta http-equiv=refresh content=0;url=//x.test><base href=//x.test>"></noscript>',
      '<form><math><mtext></form><form><mglyph><style></math><meta http-equiv=refresh content=0;url=//x.test>',
    ];
    for (const source of payloads) {
      const page = parsed(source);
      expect(page.querySelectorAll("meta[http-equiv]")).toHaveLength(1);
      expect(page.querySelectorAll("base[href]")).toHaveLength(0);
      expect(page.querySelectorAll("base")).toHaveLength(1);
      expect(page.querySelectorAll("iframe, object, embed, template, link")).toHaveLength(0);
    }
  });

  it("a page that never settles is shown as inert text, not as markup", () => {
    const original = DOMParser.prototype.parseFromString;
    let pass = 0;
    const spy = vi.spyOn(DOMParser.prototype, "parseFromString").mockImplementation(function (this: DOMParser, text, type) {
      const page = original.call(this, text, type);
      if (text !== "<pre></pre>") page.body.append(page.createComment(String(pass++)));
      return page;
    });
    try {
      const out = previewDocument("<img src=x onerror=boom()>");
      expect(out).toContain("&lt;img src=x onerror=boom()&gt;");
      expect(out).toContain("<pre>");
    } finally {
      spy.mockRestore();
    }
  });

  it("sets the only base itself, pointing every link at a new context the sandbox withholds", () => {
    const bases = parsed('<base href="https://x.test/"><p>x</p>').querySelectorAll("base");
    expect(bases).toHaveLength(1);
    expect(bases[0].getAttribute("target")).toBe("_blank");
    expect(bases[0].hasAttribute("href")).toBe(false);
  });

  it("survives a fragment, an empty text and a document with no head or html", () => {
    for (const source of ["<p>just a fragment</p>", "", "plain text", "<body>no head</body>"]) {
      const page = parsed(source);
      expect(page.head.firstElementChild?.getAttribute("content")).toBe(POLICY);
    }
    expect(parsed("<p>just a fragment</p>").body.textContent).toBe("just a fragment");
  });

  it("keeps a doctype only when the source had one", () => {
    expect(previewDocument("<!DOCTYPE html><p>x</p>").startsWith("<!doctype html><meta")).toBe(true);
    expect(previewDocument("<p>x</p>")).not.toContain("<!doctype");
  });

  it("leaves the page's markup and inline styles in place", () => {
    const page = parsed('<h1 style="color:red">Title</h1><p class="a">text</p>');
    expect(page.querySelector("h1")?.getAttribute("style")).toBe("color:red");
    expect(page.querySelector("p.a")?.textContent).toBe("text");
  });
});
