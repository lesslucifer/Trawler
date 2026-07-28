import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { StatusPill } from "./StatusPill";

const toneOf = (status: number | null) => {
  const { container } = render(<StatusPill status={status} />);
  return container.querySelector("span > span")?.className ?? "";
};

describe("StatusPill", () => {
  it.each([200, 201, 204])("%i is ok", (status) => {
    expect(toneOf(status)).toContain("text-ok");
  });

  it.each([404, 500, 418, 503])("%i is warn", (status) => {
    expect(toneOf(status)).toContain("text-warn");
  });

  it.each([301, 101, 302, 999])("%i is muted", (status) => {
    expect(toneOf(status)).toContain("text-ink-mute");
  });

  it("null is pending: an ellipsis in muted tone, never a false zero", () => {
    render(<StatusPill status={null} />);

    expect(screen.getByText("…")).toBeInTheDocument();
    expect(toneOf(null)).toContain("text-ink-mute");
  });

  it("the reason phrase reaches both title and aria-label", () => {
    render(<StatusPill status={404} />);

    const labelled = screen.getByLabelText("404 Not Found");
    expect(labelled).toHaveAttribute("title", "404 Not Found");
  });

  it("an unknown code still gets alt text rather than undefined", () => {
    render(<StatusPill status={799} />);
    expect(screen.getByLabelText("799 unknown")).toBeInTheDocument();
  });

  it("pending's alt text says pending", () => {
    render(<StatusPill status={null} />);
    expect(screen.getByLabelText("pending")).toBeInTheDocument();
  });

  it("composes TonedText — the mono tabular span comes from the primitive", () => {
    render(<StatusPill status={200} />);
    expect(screen.getByText("200").className).toContain("font-mono");
    expect(screen.getByText("200").className).toContain("tabular-nums");
  });
});
