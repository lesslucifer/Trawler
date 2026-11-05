import { afterEach, describe, expect, it, vi } from "vitest";
import { hasHtmlSignature, isWellFormedXml, labelKind, normalizeLabel, readKind } from "./contentKind";

const bytes = (text: string) => new TextEncoder().encode(text);
const BOM = "﻿";
const JSON_DOC = '{"user":{"id":7,"tags":["a","b"]}}';
const HTML_DOC = "<!DOCTYPE html><html><head><title>t</title></head><body><p>hi</p></body></html>";
const XML_DOC = '<?xml version="1.0"?><envelope><body id="1"/></envelope>';
const PNG = Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a);
const BINARY = Uint8Array.of(0x00, 0x01, 0x02, 0xfe, 0xff);

afterEach(() => vi.restoreAllMocks());

describe("a label names a kind, or nothing when it is generic or unknown", () => {
  it.each([
    ["application/json", "json"],
    ["text/json", "json"],
    ["application/problem+json", "json"],
    ["application/x-www-form-urlencoded", "form"],
    ["application/xml", "xml"],
    ["text/xml", "xml"],
    ["application/atom+xml", "xml"],
    ["image/svg+xml", "xml"],
    ["text/html", "html"],
    ["application/xhtml+xml", "html"],
    ["application/javascript", "javascript"],
    ["text/javascript", "javascript"],
    ["application/x-javascript", "javascript"],
    ["application/ecmascript", "javascript"],
    ["text/ecmascript", "javascript"],
    ["text/css", "css"],
    ["image/png", "image"],
    ["image/webp", "image"],
  ])("%s → %s", (label, kind) => {
    expect(labelKind(label)).toBe(kind);
  });

  it.each(["text/plain", "application/octet-stream", "binary/octet-stream", "text/csv", "application/pdf", "", "  ", null, undefined])(
    "%j is generic",
    (label) => {
      expect(labelKind(label)).toBeNull();
    },
  );

  it("parameters and case do not change the kind", () => {
    expect(labelKind("Application/JSON; charset=UTF-8")).toBe("json");
    expect(labelKind("TEXT/HTML;charset=iso-8859-1")).toBe("html");
    expect(normalizeLabel(" Text/CSS ; charset=utf-8")).toBe("text/css");
    expect(normalizeLabel("; charset=utf-8")).toBeNull();
  });
});

describe("a whole JSON object or array beats every text label", () => {
  it.each([
    [undefined],
    ["text/plain"],
    ["application/octet-stream"],
    ["text/html"],
    ["application/xml"],
    ["text/javascript"],
    ["text/css"],
    ["application/x-www-form-urlencoded"],
  ])("JSON served as %s is json, read from its bytes", (label) => {
    expect(readKind(bytes(JSON_DOC), label)).toEqual({ kind: "json", by: "bytes", label: label ?? null });
  });

  it("JSON under a JSON label is json by its label", () => {
    expect(readKind(bytes(` \n[1,2,{"a":null}]`), "application/json; charset=utf-8")).toEqual({
      kind: "json",
      by: "label",
      label: "application/json",
    });
  });

  it.each([["42"], ['"a string"'], ["null"], ["true"], ["{oops"], ["[1,2"], [""], ["   \n\t"]])(
    "%j with no label stays text",
    (text) => {
      expect(readKind(bytes(text))).toEqual({ kind: "text", by: "bytes", label: null });
    },
  );

  it("text that only starts like JSON stays text under text/plain", () => {
    expect(readKind(bytes("{oops"), "text/plain").kind).toBe("text");
  });

  it("a JSON label over broken JSON stays json, so the surface can say why", () => {
    expect(readKind(bytes('{"a":'), "application/json")).toEqual({ kind: "json", by: "label", label: "application/json" });
  });
});

describe("an html signature reads as html", () => {
  it.each([
    ["<!DOCTYPE html><p>x</p>"],
    ["<!doctype HTML>"],
    ["<html lang=en>"],
    ["<HTML>"],
    ["<head><title>x</title></head>"],
    ["<body/>"],
    ["\n\t  <!-- served by nginx --><!-- twice -->\n<!doctype html>"],
    ['<?xml version="1.0"?>\n<!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml"></html>'],
  ])("%j", (text) => {
    expect(hasHtmlSignature(text)).toBe(true);
  });

  it.each([["<htmlish>"], ["<bodyguard/>"], ["<p>no document</p>"], ["text <html>"], ["<!-- never closed <html>"], [""]])(
    "%j is not",
    (text) => {
      expect(hasHtmlSignature(text)).toBe(false);
    },
  );

  it.each([[undefined], ["application/octet-stream"], ["text/plain"], ["application/json"]])(
    "an html page served as %s is html, read from its bytes",
    (label) => {
      expect(readKind(bytes(HTML_DOC), label)).toEqual({ kind: "html", by: "bytes", label: label ?? null });
    },
  );

  it("a BOM before the doctype does not hide it", () => {
    expect(readKind(bytes(BOM + HTML_DOC)).kind).toBe("html");
  });

  it("XHTML served as text/html is html by its label", () => {
    const xhtml = '<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body/></html>';
    expect(readKind(bytes(xhtml), "text/html")).toEqual({ kind: "html", by: "label", label: "text/html" });
  });

  it("anything under an html label is html, short of JSON, binary or an image", () => {
    expect(readKind(bytes("hello"), "text/html").kind).toBe("html");
    expect(readKind(bytes("<p>fragment</p>"), "text/html").kind).toBe("html");
  });
});

describe("well-formed XML reads as xml", () => {
  it("XML with no label or a generic one is xml, read from its bytes", () => {
    expect(readKind(bytes(XML_DOC))).toEqual({ kind: "xml", by: "bytes", label: null });
    expect(readKind(bytes(XML_DOC), "text/plain")).toEqual({ kind: "xml", by: "bytes", label: "text/plain" });
  });

  it("XML under a JSON label is xml", () => {
    expect(readKind(bytes("<a><b/></a>"), "application/json").kind).toBe("xml");
  });

  it("an xml label keeps well-formed XML, even XHTML", () => {
    const xhtml = '<html xmlns="http://www.w3.org/1999/xhtml"><body/></html>';
    expect(readKind(bytes(xhtml), "application/xml")).toEqual({ kind: "xml", by: "label", label: "application/xml" });
  });

  it("an xml label over a page that is not well-formed but signs as html is html", () => {
    expect(readKind(bytes("<!doctype html><p>open<br></p>"), "text/xml").kind).toBe("html");
  });

  it("an xml label over broken markup without an html signature stays xml", () => {
    expect(readKind(bytes("<a><b></a>"), "application/xml")).toEqual({ kind: "xml", by: "label", label: "application/xml" });
  });

  it("markup that is neither well-formed XML nor html is text without a label", () => {
    expect(readKind(bytes("<a><b></a>")).kind).toBe("text");
    expect(readKind(bytes("<p>open<br></p>")).kind).toBe("text");
  });

  it("well-formedness is judged only on text that starts with a tag", () => {
    expect(isWellFormedXml("  <ok/>")).toBe(true);
    expect(isWellFormedXml("<a><b></a>")).toBe(false);
    expect(isWellFormedXml("not markup <a/>")).toBe(false);
  });
});

describe("javascript, css and form come only from their label", () => {
  it.each([
    ["function f(){return 1}", "text/javascript", "javascript"],
    ["!function(){}();", "application/javascript", "javascript"],
    ["body{margin:0}", "text/css", "css"],
    ["a=1&b=2", "application/x-www-form-urlencoded", "form"],
    ["<b>not markup to a script</b>", "application/javascript", "javascript"],
  ])("%j under %s is %s by its label", (text, label, kind) => {
    expect(readKind(bytes(text), label)).toEqual({ kind, by: "label", label });
  });

  it.each([["function f(){return 1}"], ["body{margin:0}"], ["a=1&b=2"]])("%j with no label is text", (text) => {
    expect(readKind(bytes(text)).kind).toBe("text");
  });
});

describe("images and binary bytes", () => {
  it("an image signature is an image whatever the label", () => {
    expect(readKind(PNG, "text/plain")).toEqual({ kind: "image", by: "bytes", label: "text/plain" });
    expect(readKind(PNG)).toEqual({ kind: "image", by: "bytes", label: null });
  });

  it("an image label is never overruled", () => {
    expect(readKind(bytes(JSON_DOC), "image/png")).toEqual({ kind: "image", by: "label", label: "image/png" });
    expect(readKind(BINARY, "image/webp").kind).toBe("image");
  });

  it.each([["application/json"], ["text/html"], ["text/javascript"], [undefined]])("binary bytes under %s are hex", (label) => {
    expect(readKind(BINARY, label)).toEqual({ kind: "hex", by: "bytes", label: label ?? null });
  });

  it("text with control bytes is hex", () => {
    expect(readKind(bytes("{\u0001}"), "application/json").kind).toBe("hex");
  });
});

describe("each document is parsed at most once", () => {
  it("a 10 MiB JSON document calls JSON.parse exactly once and never the XML parser", () => {
    const row = '{"id":12345,"name":"a fairly long name","tags":["x","y","z"]},';
    const big = `[${row.repeat(Math.ceil((10 * 1024 * 1024) / row.length))}{}]`;
    const parse = vi.spyOn(JSON, "parse");
    const xml = vi.spyOn(DOMParser.prototype, "parseFromString");
    expect(readKind(bytes(big), "text/plain").kind).toBe("json");
    expect(parse).toHaveBeenCalledTimes(1);
    expect(xml).not.toHaveBeenCalled();
  });

  it("text that does not start like JSON or markup reaches neither parser", () => {
    const parse = vi.spyOn(JSON, "parse");
    const xml = vi.spyOn(DOMParser.prototype, "parseFromString");
    expect(readKind(bytes("plain words, nothing more")).kind).toBe("text");
    expect(parse).not.toHaveBeenCalled();
    expect(xml).not.toHaveBeenCalled();
  });
});
