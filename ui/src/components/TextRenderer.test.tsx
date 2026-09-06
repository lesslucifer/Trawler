import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { byteLength, TEXT_MAX } from "../body";
import { TextRenderer } from "./TextRenderer";

const UNITS: Record<string, number> = { B: 1, KiB: 1024, MiB: 1024 ** 2, GiB: 1024 ** 3 };

function sizes(marker: string): [number, number] {
  const found = [...marker.matchAll(/([\d.]+) (B|KiB|MiB|GiB)/g)];
  expect(found).toHaveLength(2);
  const [shown, total] = found.map(([, value, unit]) => Number(value) * UNITS[unit]);
  return [shown, total];
}

describe("TextRenderer", () => {
  it("renders the text verbatim — no reformatting of what arrived", () => {
    const text = 'Request failed: upstream timed out\n  at Gateway.forward (forward.js:118:11)';
    render(<TextRenderer text={text} />);

    expect(screen.getByTestId("body-text").textContent).toBe(text);
  });

  it("wraps long lines instead of scrolling the panel away", () => {
    render(<TextRenderer text={"x".repeat(500)} />);

    const pre = screen.getByTestId("body-text");
    expect(pre.className).toContain("whitespace-pre-wrap");
    expect(pre.className).toContain("break-all");
    expect(pre.className).not.toContain("truncate");
  });

  it("applies no syntax tokens — it has no grammar to apply them to", () => {
    const { container } = render(<TextRenderer text='{"looks":"like json"}' />);

    expect(container.querySelector("[class*=text-syn-]")).toBeNull();
  });

  it("mounts no editor surface: plain text has nothing to navigate or fold", () => {
    const { container } = render(<TextRenderer text="plain" />);

    expect(container.querySelector(".cm-editor")).toBeNull();
    expect(container.querySelector(".cm-gutters")).toBeNull();
  });

  it("renders a body at the cap whole, with no marker", () => {
    render(<TextRenderer text={"a".repeat(TEXT_MAX)} />);

    expect(screen.getByTestId("body-text").textContent).toHaveLength(TEXT_MAX);
    expect(screen.queryByText(/showing/)).not.toBeInTheDocument();
  });

  it("caps an over-large body and says so — the defect this component's spec fixes", () => {
    render(<TextRenderer text={"a".repeat(TEXT_MAX + 5000)} />);

    expect(screen.getByText(/⚠ showing 128 KiB of 133 KiB — save to see all/)).toBeInTheDocument();
  });

  // The fetch that produced the text already counted its bytes; re-deriving the
  // count means encoding the whole body to read one number off it.
  it("states the size its caller already knew", () => {
    const text = "é".repeat(TEXT_MAX + 5000);
    render(<TextRenderer text={text} size={2 * (TEXT_MAX + 5000)} />);

    expect(screen.getByText(/⚠ showing 256 KiB of 266 KiB — save to see all/)).toBeInTheDocument();
  });

  it("derives the same size when its caller cannot supply one", () => {
    const text = "é".repeat(TEXT_MAX + 5000);

    const supplied = render(<TextRenderer text={text} size={byteLength(text)} />);
    const derived = render(<TextRenderer text={text} />);

    expect(derived.container.textContent).toBe(supplied.container.textContent);
  });

  // A text/*-labelled body whose bytes are not valid UTF-8 decodes to one U+FFFD
  // per unreadable byte, and U+FFFD re-encodes to three bytes — so a shown slice
  // measured by re-encoding can exceed the whole body's real size. "Showing more
  // than there is" is the one statement this marker exists to prevent.
  it("never states a shown size larger than the total", () => {
    const bytes = new Uint8Array(TEXT_MAX + 5000).fill(0xff);
    render(<TextRenderer text={new TextDecoder().decode(bytes)} size={bytes.length} />);

    const [shown, total] = sizes(screen.getByText(/⚠ showing/).textContent ?? "");
    expect(shown).toBeLessThanOrEqual(total);
  });

  it("states the cut in warn tone, never a silent ellipsis", () => {
    render(<TextRenderer text={"a".repeat(TEXT_MAX + 1)} />);

    expect(screen.getByText(/showing/).className).toContain("text-warn");
  });
});
