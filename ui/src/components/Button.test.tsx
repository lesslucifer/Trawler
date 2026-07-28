import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Button } from "./Button";

describe("Button", () => {
  it("renders a labelled action whose label is its accessible name", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>retry</Button>);

    const button = screen.getByRole("button", { name: "retry" });
    fireEvent.click(button);

    expect(onClick).toHaveBeenCalledOnce();
    expect(button).toHaveAttribute("type", "button");
  });

  it("in-flight is disabled: no click, out of tab order (fork D4 — no loading prop exists)", () => {
    const onClick = vi.fn();
    render(
      <Button onClick={onClick} disabled>
        retry
      </Button>,
    );

    const button = screen.getByRole("button", { name: "retry" });
    expect(button).toBeDisabled();

    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("defaults to Button's own values on all three axes", () => {
    render(<Button onClick={() => {}}>retry</Button>);

    const button = screen.getByRole("button");
    expect(button.className).toContain("border-hairline");
    expect(button.className).toContain("text-ink-body");
    expect(button.className).toContain("min-h-control");
  });

  it("frame=bare drops the border and hovers as a wash instead", () => {
    render(
      <Button onClick={() => {}} frame="bare">
        x
      </Button>,
    );

    const button = screen.getByRole("button");
    expect(button.className).toContain("border-none");
    expect(button.className).toContain("hover:bg-wash-hover");
  });

  it("shape=square takes a size-px box and keeps the 24px WCAG floor", () => {
    const { rerender } = render(
      <Button onClick={() => {}} shape="square" size={28} label="close">
        ×
      </Button>,
    );
    expect(screen.getByRole("button", { name: "close" })).toHaveStyle({
      width: "28px",
      height: "28px",
    });

    rerender(
      <Button onClick={() => {}} shape="square" size={12} label="close">
        ×
      </Button>,
    );
    expect(screen.getByRole("button", { name: "close" })).toHaveStyle({
      width: "24px",
      height: "24px",
    });
  });

  it("tone=mute drops the resting colour one step; hover converges on ink", () => {
    render(
      <Button onClick={() => {}} tone="mute">
        x
      </Button>,
    );

    const button = screen.getByRole("button");
    expect(button.className).toContain("text-ink-mute");
    expect(button.className).toContain("hover:text-ink");
  });

  it("carries the focus ring and disabled treatment its siblings inherit", () => {
    render(<Button onClick={() => {}}>retry</Button>);

    const button = screen.getByRole("button");
    expect(button.className).toContain("focus-visible:outline-ink-mute");
    expect(button.className).toContain("disabled:opacity-disabled");
  });

  it("the optional glyph is aria-hidden — the label stays the accessible name", () => {
    render(
      <Button onClick={() => {}} glyph="↻">
        retry
      </Button>,
    );

    expect(screen.getByRole("button", { name: "retry" })).toBeInTheDocument();
    expect(screen.getByText("↻")).toHaveAttribute("aria-hidden", "true");
  });
});
