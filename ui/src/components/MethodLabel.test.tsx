import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { MethodLabel } from "./MethodLabel";

const classOf = (method: string) => {
  const { container } = render(<MethodLabel method={method} />);
  return container.querySelector("span")?.className ?? "";
};

describe("MethodLabel", () => {
  it.each([
    ["GET", "text-ok"],
    ["POST", "text-create"],
    ["PUT", "text-replace"],
    ["PATCH", "text-amend"],
    ["DELETE", "text-warn"],
  ])("%s carries the tone of what it does to the resource", (method, tone) => {
    expect(classOf(method)).toContain(tone);
  });

  it.each(["HEAD", "OPTIONS", "CONNECT", "TRACE", "PROPFIND"])(
    "%s falls back to body ink rather than inventing a tone",
    (method) => {
      const className = classOf(method);
      expect(className).toContain("text-ink-body");
      expect(className).not.toMatch(/text-(ok|create|replace|amend|warn)\b/);
    },
  );

  it("draws every tone from the closed palette — no method-named token survives", () => {
    for (const method of ["GET", "POST", "PUT", "PATCH", "DELETE", "PROPFIND"]) {
      expect(classOf(method)).not.toContain("text-method-");
    }
  });

  it("renders the verb uppercase whatever the API's casing", () => {
    render(<MethodLabel method="get" />);
    expect(screen.getByText("GET")).toBeInTheDocument();
  });

  it("holds a fixed-width slot, so a 7-char verb never pushes the next column", () => {
    expect(classOf("OPTIONS")).toContain("min-w-verb");
    expect(classOf("GET")).toContain("min-w-verb");
  });

  it("never truncates or abbreviates the verb — the wire word is the whole word", () => {
    render(<MethodLabel method="PROPFIND" />);

    const label = screen.getByText("PROPFIND");
    expect(label.className).not.toContain("truncate");
    expect(label.textContent).toBe("PROPFIND");
  });
});
