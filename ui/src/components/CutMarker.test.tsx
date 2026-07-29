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

  // Found in the gallery: TREE_MAX vs TREE_MAX+11 both rounded to "128 KiB", so
  // a real truncation read as none — the precise failure this line prevents.
  it("never reads as showing everything when it is not", () => {
    render(<CutMarker shown={131_072} total={131_083} />);

    const [, shown, total] =
      screen.getByText(/showing/).textContent!.match(/showing (.+) of (.+) — save/)!;
    expect(shown).not.toBe(total);
    expect(shown).toBe("128.00 KiB");
    expect(total).toBe("128.01 KiB");
  });

  it("borrows only as much precision as it needs to tell them apart", () => {
    render(<CutMarker shown={4096} total={148_480} />);
    expect(screen.getByText(/showing 4.0 KiB of 145 KiB/)).toBeInTheDocument();
  });

  it("never states raw byte counts", () => {
    render(<CutMarker shown={138_240} total={1_048_576} />);

    expect(screen.getByText(/showing 135 KiB of 1.0 MiB/)).toBeInTheDocument();
    expect(screen.queryByText(/138240|1048576/)).not.toBeInTheDocument();
  });
});
