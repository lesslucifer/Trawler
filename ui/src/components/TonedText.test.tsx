import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { TonedText } from "./TonedText";

describe("TonedText", () => {
  it.each([
    ["ok", "text-ok"],
    ["warn", "text-warn"],
    ["muted", "text-ink-mute"],
  ] as const)("tone %s resolves to %s", (tone, expected) => {
    render(<TonedText tone={tone}>value</TonedText>);
    expect(screen.getByText("value").className).toContain(expected);
  });

  it("is a mono tabular span and nothing else — no box, no role, no title", () => {
    render(<TonedText tone="ok">200</TonedText>);

    const span = screen.getByText("200");
    expect(span.tagName).toBe("SPAN");
    expect(span.className).toContain("font-mono");
    expect(span.className).toContain("tabular-nums");
    expect(span).not.toHaveAttribute("title");
    expect(span).not.toHaveAttribute("role");
  });

  it("never inspects its children — no business logic (the reason it is T1)", () => {
    render(<TonedText tone="warn">200</TonedText>);

    const span = screen.getByText("200");
    expect(span.className).toContain("text-warn");
    expect(span.className).not.toContain("text-ok");
  });

  it("renders children as given, including non-text nodes", () => {
    render(
      <TonedText tone="ok">
        <em>✓ trusted</em>
      </TonedText>,
    );
    expect(screen.getByText("✓ trusted").tagName).toBe("EM");
  });
});
