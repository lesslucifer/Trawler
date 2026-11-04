import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { HEX_MAX } from "../body";
import { HexRenderer } from "./HexRenderer";

const PNG_HEAD = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

describe("HexRenderer", () => {
  it("renders offset, hex bytes and the ASCII gutter", () => {
    render(<HexRenderer bytes={PNG_HEAD} />);

    const dump = screen.getByTestId("body-hex").textContent ?? "";
    expect(dump).toContain("00000000");
    expect(dump).toContain("89 50 4e 47 0d 0a 1a 0a");
    expect(dump).toContain("|.PNG....|");
  });

  it("tones the three parts apart — offset mute, bytes body, gutter ink", () => {
    render(<HexRenderer bytes={PNG_HEAD} />);

    expect(screen.getByText("00000000").className).toContain("text-ink-mute");
    expect(screen.getByText("|.PNG....|").className).toContain("text-ink");
  });

  it("shows non-printable bytes as dots, never as control characters", () => {
    render(<HexRenderer bytes={new Uint8Array([0x00, 0x01, 0x7f, 0x41])} />);
    expect(screen.getByText("|...A|")).toBeInTheDocument();
  });

  it("lays 16 bytes to a line", () => {
    render(<HexRenderer bytes={new Uint8Array(20).fill(0x41) } />);

    const dump = screen.getByTestId("body-hex").textContent ?? "";
    expect(dump).toContain("00000000");
    expect(dump).toContain("00000010");
  });

  it("caps at HEX_MAX and states the cut in the family's form", () => {
    render(<HexRenderer bytes={new Uint8Array(HEX_MAX + 10_240).fill(0x41)} />);

    expect(screen.getByText(/showing/).textContent).toBe(
      "⚠ showing 4.0 KiB of 14 KiB — save to see all",
    );
  });

  it("never falls back to the muted ellipsis hexDump appends by default", () => {
    render(<HexRenderer bytes={new Uint8Array(HEX_MAX + 100).fill(0x41)} />);

    expect(screen.queryByText(/more bytes/)).not.toBeInTheDocument();
    expect(screen.getByText(/showing/).parentElement!.className).toContain("text-warn");
  });

  it("shows no marker for a body inside the cap", () => {
    render(<HexRenderer bytes={PNG_HEAD} />);
    expect(screen.queryByText(/showing/)).not.toBeInTheDocument();
  });
});
