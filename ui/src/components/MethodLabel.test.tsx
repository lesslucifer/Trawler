import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { MethodLabel } from "./MethodLabel";

const classOf = (method: string) => {
  const { container } = render(<MethodLabel method={method} />);
  return container.querySelector("span")?.className ?? "";
};

describe("MethodLabel", () => {
  it.each([
    ["GET", "text-method-get"],
    ["POST", "text-method-post"],
    ["PUT", "text-method-put"],
    ["DELETE", "text-method-delete"],
    ["PATCH", "text-method-patch"],
  ])("%s carries its own token", (method, token) => {
    expect(classOf(method)).toContain(token);
  });

  it.each(["HEAD", "OPTIONS", "CONNECT", "TRACE", "PROPFIND"])(
    "%s falls back to body tone rather than inventing a colour",
    (method) => {
      const className = classOf(method);
      expect(className).toContain("text-ink-body");
      expect(className).not.toContain("text-method-");
    },
  );

  it("renders the verb uppercase whatever the API's casing", () => {
    render(<MethodLabel method="get" />);
    expect(screen.getByText("GET")).toBeInTheDocument();
  });

  it("holds a fixed-width slot, so a 7-char verb never pushes the next column", () => {
    expect(classOf("OPTIONS")).toContain("min-w-[4.5ch]");
    expect(classOf("GET")).toContain("min-w-[4.5ch]");
  });

  it("never truncates or abbreviates the verb — the wire word is the whole word", () => {
    render(<MethodLabel method="PROPFIND" />);

    const label = screen.getByText("PROPFIND");
    expect(label.className).not.toContain("truncate");
    expect(label.textContent).toBe("PROPFIND");
  });
});
