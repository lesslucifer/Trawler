import { describe, expect, it } from "vitest";
import { DATA_KINDS, kindLabel, signatureOf } from "./dataKind";

it("the picker offers the nine kinds in order, each labelled for the status bar", () => {
  expect(DATA_KINDS.map((k) => k.label)).toEqual(["JSON", "form", "XML", "HTML", "JavaScript", "CSS", "text", "hex", "image"]);
  expect(kindLabel("html")).toBe("HTML");
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
    }
    expect(signatureOf(Uint8Array.of(0x00, 0x01))).toBeNull();
  });
});
