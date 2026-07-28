import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Button } from "./Button";
import { Notice } from "./Notice";

describe("Notice", () => {
  it("defaults to mute with no glyph — the absence case is the common one", () => {
    const { container } = render(<Notice>no headers on this leg</Notice>);

    expect(screen.getByText("no headers on this leg").className).toContain("text-ink-mute");
    expect(container.querySelector('[aria-hidden="true"]')).not.toBeInTheDocument();
  });

  it("warn adds ⚠ as a second channel, so colour is never alone", () => {
    render(<Notice tone="warn">could not load this flow</Notice>);

    expect(screen.getByText("could not load this flow").className).toContain("text-warn");
    expect(screen.getByText("⚠")).toHaveAttribute("aria-hidden", "true");
  });

  it("ok adds ✓ — the positive outcome this WU permits (D4)", () => {
    render(<Notice tone="ok">trawler is capturing</Notice>);

    expect(screen.getByText("trawler is capturing").className).toContain("text-ok");
    expect(screen.getByText("✓")).toHaveAttribute("aria-hidden", "true");
  });

  it("the optional detail line stays mute under every tone", () => {
    const { rerender } = render(
      <Notice tone="warn" detail="the proxy stopped responding">
        could not load
      </Notice>,
    );
    expect(screen.getByText("the proxy stopped responding").className).toContain("text-ink-mute");

    rerender(
      <Notice tone="ok" detail="12 flows so far">
        capturing
      </Notice>,
    );
    expect(screen.getByText("12 flows so far").className).toContain("text-ink-mute");
  });

  it("the action slot holds the way out and stays clickable", () => {
    const onClick = vi.fn();
    render(
      <Notice tone="warn" action={<Button onClick={onClick}>retry</Button>}>
        could not load this flow
      </Notice>,
    );

    fireEvent.click(screen.getByRole("button", { name: "retry" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("a button in the action slot stays ink-toned inside a warn notice (Button D2)", () => {
    render(
      <Notice tone="warn" action={<Button onClick={() => {}}>retry</Button>}>
        could not load
      </Notice>,
    );

    const button = screen.getByRole("button", { name: "retry" });
    expect(button.className).toContain("text-ink-body");
    expect(button.className).not.toContain("text-warn");
  });

  it("omits the action slot entirely when nothing would help", () => {
    render(<Notice>this flow is no longer captured</Notice>);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("carries no panel chrome of its own — no border, background or radius", () => {
    const { container } = render(<Notice>nothing here</Notice>);

    const root = container.firstElementChild;
    expect(root?.className ?? "").not.toContain("border");
    expect(root?.className ?? "").not.toContain("rounded");
    expect(root?.className ?? "").not.toContain("bg-");
  });

  it("is not interactive and cannot be dismissed", () => {
    render(<Notice tone="warn">could not load</Notice>);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
