import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import jsQR from "jsqr";
import { QrCode } from "./QrCode";

// What a camera sees: the drawn modules, dark on the light plate, rasterised at
// a few pixels each with the plate's light margin around them.
function decode(svg: Element): string | null {
  const size = Number(svg.getAttribute("viewBox")!.split(" ")[2]);
  const dark = new Set(
    [...(svg.querySelector("path")!.getAttribute("d") ?? "").matchAll(/M(\d+) (\d+)/g)].map(
      ([, x, y]) => `${x},${y}`,
    ),
  );
  const scale = 4;
  const side = size * scale;
  const pixels = new Uint8ClampedArray(side * side * 4);
  for (let py = 0; py < side; py++)
    for (let px = 0; px < side; px++) {
      const shade = dark.has(`${Math.floor(px / scale)},${Math.floor(py / scale)}`) ? 0 : 255;
      pixels.set([shade, shade, shade, 255], (py * side + px) * 4);
    }
  return jsQR(pixels, side, side)?.data ?? null;
}

const CA_URL = "http://192.168.1.24:7421/ca.mobileconfig";

describe("QrCode", () => {
  it("draws a code that decodes to exactly the value it was given", () => {
    render(<QrCode value={CA_URL} label="the certificate URL" />);

    expect(decode(screen.getByRole("img"))).toBe(CA_URL);
  });

  it("encodes the longest address a phone is handed — an IPv6 certificate URL", () => {
    const v6 = "http://[2001:db8:85a3::8a2e:370:7334]:7421/ca.mobileconfig";
    render(<QrCode value={v6} label="the certificate URL" />);

    expect(decode(screen.getByRole("img"))).toBe(v6);
  });

  it("is named for the value it carries, which a reader finds printed beside it", () => {
    render(<QrCode value={CA_URL} label="the certificate URL" />);

    expect(screen.getByRole("img", { name: "the certificate URL, as a code to scan" })).toHaveAttribute(
      "data-value",
      CA_URL,
    );
  });

  it("redraws when the value changes, never keeping the old code", () => {
    const { rerender } = render(<QrCode value={CA_URL} label="the certificate URL" />);
    const other = "http://10.0.0.7:7421/ca.cer";
    rerender(<QrCode value={other} label="the certificate URL" />);

    expect(decode(screen.getByRole("img"))).toBe(other);
  });
});
