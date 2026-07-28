import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { CutMarker } from "./CutMarker";

describe("CutMarker", () => {
  it("states what is shown, of how much, and the way out", () => {
    render(<CutMarker shown={4096} total={14_848} />);

    expect(screen.getByText(/showing/).textContent).toBe(
      "⚠ showing 4.0 KiB of 15 KiB — save to see all",
    );
  });

  it("carries the warn tone — a truncation the user misses is a truncation that lies", () => {
    render(<CutMarker shown={4096} total={99_999} />);
    expect(screen.getByText(/showing/).className).toContain("text-warn");
  });

  it("never states raw byte counts", () => {
    render(<CutMarker shown={138_240} total={1_048_576} />);

    expect(screen.getByText(/showing 135 KiB of 1.0 MiB/)).toBeInTheDocument();
    expect(screen.queryByText(/138240|1048576/)).not.toBeInTheDocument();
  });
});
