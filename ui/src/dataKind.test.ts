import { describe, expect, it } from "vitest";
import { detectKind, FROM_BYTES, kindLabel, signatureOf } from "./dataKind";

const bytes = (text: string) => new TextEncoder().encode(text);

describe("the hint names the kind when it names one", () => {
  it.each([
    ["application/json; charset=utf-8", "json", "application/json"],
    ["application/problem+json", "json", "application/problem+json"],
    ["application/x-www-form-urlencoded", "form", "application/x-www-form-urlencoded"],
    ["text/html", "xml", "text/html"],
    ["text/plain", "text", "text/plain"],
    ["image/png", "image", "image/png"],
  ])("%s → %s", (hint, kind, from) => {
    expect(detectKind(hint, bytes("whatever"))).toEqual({ kind, from });
  });

  it("a hint that only says bytes leaves the bytes to speak", () => {
    expect(detectKind("application/octet-stream", bytes('{"a":1}'))).toEqual({ kind: "json", from: FROM_BYTES });
  });
});

describe("without a hint the bytes are sniffed", () => {
  it.each([
    ['  {"a":[1,2]}', "json"],
    ["[1,2", "text"],
    ["<envelope/>", "xml"],
    ["plain words", "text"],
    ["", "text"],
  ])("%j → %s", (text, kind) => {
    expect(detectKind(undefined, bytes(text)).kind).toBe(kind);
  });

  it("a PNG, JPEG or GIF signature is an image", () => {
    expect(detectKind(undefined, new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0, 1])).kind).toBe("image");
    expect(detectKind(undefined, new Uint8Array([0xff, 0xd8, 0xff, 0xe0])).kind).toBe("image");
    expect(detectKind("", new Uint8Array([0x47, 0x49, 0x46, 0x38, 0x39])).kind).toBe("image");
  });

  it("bytes that are not text are hex", () => {
    expect(detectKind(undefined, new Uint8Array([0, 1, 2, 0xfe]))).toEqual({ kind: "hex", from: FROM_BYTES });
  });
});

it("every kind has the label the status bar shows", () => {
  expect(kindLabel("xml")).toBe("XML / HTML");
  expect(kindLabel("json")).toBe("JSON");
});

describe("one signature table names the bytes and says which are images", () => {
  it("names PNG, JPEG, GIF, gzip and PDF, and only the first three sniff as image", () => {
    const cases: [number[], string, boolean][] = [
      [[0x89, 0x50, 0x4e, 0x47, 0x0d], "PNG", true],
      [[0xff, 0xd8, 0xff, 0xe0], "JPEG", true],
      [[0x47, 0x49, 0x46, 0x38, 0x39], "GIF", true],
      [[0x1f, 0x8b, 0x08], "gzip", false],
      [[0x25, 0x50, 0x44, 0x46, 0x2d], "PDF", false],
    ];
    for (const [bytes, name, image] of cases) {
      expect(signatureOf(Uint8Array.from(bytes))).toEqual({ name, image });
      expect(detectKind(undefined, Uint8Array.from(bytes)).kind === "image").toBe(image);
    }
    expect(signatureOf(Uint8Array.of(0x00, 0x01))).toBeNull();
  });
});
