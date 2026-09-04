import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { Disclosure } from "./Disclosure";

describe("Disclosure", () => {
  it("is a native <details>, so the platform owns focus, keys and find-in-page (D1, B2)", () => {
    const { container } = render(
      <Disclosure summary="set it up manually">
        <p>steps for macOS</p>
      </Disclosure>,
    );

    const details = container.querySelector("details");
    const summary = screen.getByText("set it up manually").closest("summary");
    expect(details).toBeInTheDocument();
    expect(summary?.parentElement).toBe(details);
    expect(summary).not.toHaveAttribute("role");
    expect(summary).not.toHaveAttribute("tabindex");
    expect(summary).not.toHaveAttribute("aria-expanded");
  });

  it("starts closed by default and defaultOpen seeds it open (B1)", () => {
    const { container, unmount } = render(
      <Disclosure summary="remove the certificate later">how to remove it</Disclosure>,
    );
    expect(container.querySelector("details")).not.toHaveAttribute("open");
    unmount();

    const { container: seeded } = render(
      <Disclosure summary="set it up manually" defaultOpen>
        steps
      </Disclosure>,
    );
    expect(seeded.querySelector("details")).toHaveAttribute("open");
  });

  it("seeds without binding — a re-render does not yank a closed panel back open (B1)", () => {
    const { container, rerender } = render(
      <Disclosure summary="set it up manually" defaultOpen>
        steps
      </Disclosure>,
    );

    const details = container.querySelector("details") as HTMLDetailsElement;
    details.open = false;

    rerender(
      <Disclosure summary="set it up manually" defaultOpen>
        steps
      </Disclosure>,
    );
    expect(details.open).toBe(false);
  });

  it("suppresses the UA marker and draws the app's own glyph (D3)", () => {
    render(<Disclosure summary="turn the proxy off later">how to turn it off</Disclosure>);

    const summary = screen.getByText("turn the proxy off later").closest("summary");
    expect(summary?.className).toContain("list-none");
    expect(summary?.className).toContain("[&::-webkit-details-marker]:hidden");
    expect(summary).toHaveTextContent("›");
  });

  it("rotates one glyph rather than swapping two, so the label never shifts (D2)", () => {
    const { container } = render(
      <Disclosure summary="set it up manually" defaultOpen>
        steps
      </Disclosure>,
    );

    const twist = container.querySelector("[data-twist]");
    expect(twist).toHaveTextContent("›");
    expect(twist).toHaveAttribute("aria-hidden", "true");
    expect(twist?.className).toContain("w-glyph");
    expect(twist?.className).toContain("transition-transform");
    expect(container.querySelector("details")?.className).toContain("rotate-90");
  });

  it("the summary row is the target, full width at the app's hit floor (driver ⑦)", () => {
    render(<Disclosure summary="set it up manually">steps</Disclosure>);

    const summary = screen.getByText("set it up manually").closest("summary");
    expect(summary?.className).toContain("flex");
    expect(summary?.className).toContain("min-h-control");
    expect(summary?.className).toContain("cursor-pointer");
    expect(summary?.className).toContain("select-none");
    expect(summary?.className).toContain("hover:bg-wash-hover");
  });

  it("the slot adds a gap and nothing else — no border, padding, indent or height cap (anatomy ③)", () => {
    render(
      <Disclosure summary="set it up manually">
        <p>steps for macOS</p>
      </Disclosure>,
    );

    const slot = screen.getByText("steps for macOS").parentElement;
    expect(slot?.className).toBe("mt-chrome-y");
  });

  it("never inspects what it hides — the slot renders whatever the host passed", () => {
    render(
      <Disclosure summary="set it up manually">
        <ol>
          <li>download the certificate</li>
          <li>trust it</li>
        </ol>
      </Disclosure>,
    );

    expect(screen.getByRole("list")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
  });
});
