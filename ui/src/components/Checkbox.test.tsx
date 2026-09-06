import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Checkbox } from "./Checkbox";

describe("Checkbox", () => {
  it("reports the new boolean, not the event", () => {
    const onChange = vi.fn();
    render(
      <Checkbox checked={false} onChange={onChange}>
        don&apos;t show this again
      </Checkbox>,
    );

    fireEvent.click(screen.getByRole("checkbox", { name: "don't show this again" }));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("holds no value of its own — what the caller passes is what renders", () => {
    const { rerender } = render(
      <Checkbox checked={false} onChange={() => {}}>
        remember this
      </Checkbox>,
    );
    const box = screen.getByRole("checkbox", { name: "remember this" });
    expect(box).not.toBeChecked();

    fireEvent.click(box);
    expect(box).not.toBeChecked();

    rerender(
      <Checkbox checked onChange={() => {}}>
        remember this
      </Checkbox>,
    );
    expect(box).toBeChecked();
  });

  it("puts the label inside the hit target, so clicking the words toggles it", () => {
    const onChange = vi.fn();
    render(
      <Checkbox checked={false} onChange={onChange}>
        remember this
      </Checkbox>,
    );

    const box = screen.getByRole("checkbox", { name: "remember this" });
    const label = box.closest("label");
    expect(label).not.toBeNull();
    expect(label).toHaveTextContent("remember this");

    fireEvent.click(screen.getByText("remember this"));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("disabled reports nothing and leaves the tab order", () => {
    const onChange = vi.fn();
    render(
      <Checkbox checked={false} onChange={onChange} disabled>
        remember this
      </Checkbox>,
    );

    const box = screen.getByRole("checkbox", { name: "remember this" });
    expect(box).toBeDisabled();

    fireEvent.click(box);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("shows where focus is — the ring every other focusable member carries", () => {
    render(
      <Checkbox checked={false} onChange={() => {}}>
        remember this
      </Checkbox>,
    );

    const box = screen.getByRole("checkbox", { name: "remember this" });
    expect(box.className).toContain("focus-visible:outline");
    expect(box.className).not.toContain("outline-none");
  });

  it("has no indeterminate state — checked is the whole state machine", () => {
    render(
      <Checkbox checked onChange={() => {}}>
        remember this
      </Checkbox>,
    );

    const box = screen.getByRole<HTMLInputElement>("checkbox", { name: "remember this" });
    expect(box.indeterminate).toBe(false);
    expect(box).not.toHaveAttribute("aria-checked", "mixed");
  });
});
