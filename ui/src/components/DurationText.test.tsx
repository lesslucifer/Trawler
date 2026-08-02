import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { DurationText } from "./DurationText";

const shown = (value: number | null, unit?: "ms" | "s" | "min" | "h" | "d") => {
  const { container } = render(<DurationText value={value} unit={unit} />);
  return container.textContent;
};

describe("DurationText", () => {
  it.each([
    [0, "0 ms"],
    [1, "1 ms"],
    [327, "327 ms"],
    [999, "999 ms"],
  ])("%i ms stays integer ms", (ms, expected) => {
    expect(shown(ms)).toBe(expected);
  });

  it.each([
    [1000, "1.0 s"],
    [1842, "1.8 s"],
    [1050, "1.1 s"],
    [59999, "60.0 s"],
    [60000, "1.0 min"],
    [5400000, "1.5 h"],
    [129600000, "1.5 d"],
  ])("%i ms scales to %s at its clock boundary", (ms, expected) => {
    expect(shown(ms)).toBe(expected);
  });

  it("always keeps one decimal above the threshold, so the column width is stable", () => {
    expect(shown(2000)).toBe("2.0 s");
    expect(shown(10000)).toBe("10.0 s");
  });

  it("normalizes the input unit first; the display unit is still the ladder's", () => {
    expect(shown(250)).toBe("250 ms");
    expect(shown(1.5, "s")).toBe("1.5 s");
    expect(shown(90, "s")).toBe("1.5 min");
    expect(shown(36, "h")).toBe("1.5 d");
  });

  it("pending holds the column with an invisible placeholder — never a live ticker", () => {
    const { container } = render(<DurationText value={null} />);

    const span = container.querySelector("span");
    expect(span).toHaveTextContent("000 ms");
    expect(span?.className).toContain("text-transparent");
    expect(span).toHaveAttribute("aria-hidden", "true");
  });

  it("expands the unit for a screen reader rather than spelling out 'm s'", () => {
    render(<DurationText value={327} />);
    expect(screen.getByLabelText("327 milliseconds")).toBeInTheDocument();
  });

  it("names the scaled unit in the accessible label too", () => {
    render(<DurationText value={1842} />);
    expect(screen.getByLabelText("1.8 seconds")).toBeInTheDocument();
  });

  it("is muted and tabular — never tinted ok/warn by speed", () => {
    render(<DurationText value={327} />);

    const span = screen.getByLabelText("327 milliseconds");
    expect(span.className).toContain("text-ink-mute");
    expect(span.className).toContain("tabular-nums");
    expect(span.className).not.toContain("text-warn");
  });
});
