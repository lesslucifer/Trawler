import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { MenuPath } from "./MenuPath";

const PATH = ["Trust", "When using this certificate", "Always Trust"];

const names = (container: HTMLElement) =>
  [...container.querySelectorAll("[data-testid=menu-path] > span:not(.text-ink-mute)")].map((n) => n.textContent);

describe("MenuPath", () => {
  it("reads as the path, in click order, with an arrow between names", () => {
    const { container } = render(<MenuPath path={PATH} />);

    expect(container.textContent).toBe(PATH.join(" → "));
    expect(names(container)).toEqual(PATH);
  });

  it("sets each name apart from the sentence in ink, the arrows muted", () => {
    const { container } = render(<MenuPath path={PATH} />);

    const parts = [...container.querySelectorAll("[data-testid=menu-path] > span")];
    expect(parts.filter((p) => p.textContent === "→").every((p) => p.className.includes("text-ink-mute"))).toBe(true);
    expect(parts.filter((p) => p.textContent !== "→").every((p) => p.className.includes("text-ink"))).toBe(true);
  });

  // W17's D1: a box that wraps takes the full width, so a ? after the path
  // would fall to a line of its own; text flows on after the last name.
  it("breaks between names, never inside one, flowing as text", () => {
    const { container } = render(<MenuPath path={PATH} />);

    const path = container.querySelector("[data-testid=menu-path]")!;
    expect(path.className).not.toMatch(/flex/);
    expect(path.textContent).toBe(PATH.join(" → "));
    for (const name of container.querySelectorAll("[data-testid=menu-path] > span:not(.text-ink-mute)")) {
      expect(name.className).toContain("whitespace-nowrap");
    }
  });

  it("is a label, not wire data: never mono", () => {
    const { container } = render(<MenuPath path={PATH} />);

    expect(container.innerHTML).not.toContain("font-mono");
  });

  it("names one control alone, with no arrow", () => {
    const { container } = render(<MenuPath path={["Trawler Proxy CA"]} />);

    expect(container.textContent).toBe("Trawler Proxy CA");
  });
});
