import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { SegmentedControl } from "./SegmentedControl";

const OPTIONS = [
  { value: "raw", label: "raw" },
  { value: "pretty", label: "pretty" },
] as const;

const setup = (value = "raw", disabled = false) => {
  const onChange = vi.fn();
  render(
    <SegmentedControl
      options={OPTIONS}
      value={value}
      onChange={onChange}
      ariaLabel="body view"
      disabled={disabled}
    />,
  );
  return onChange;
};

describe("SegmentedControl", () => {
  it("is a radiogroup with exactly one checked segment", () => {
    setup("raw");

    expect(screen.getByRole("radiogroup", { name: "body view" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "raw" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "pretty" })).not.toBeChecked();
  });

  it("clicking an inactive segment reports it; the active one is a no-op", () => {
    const onChange = setup("raw");

    fireEvent.click(screen.getByRole("radio", { name: "pretty" }));
    expect(onChange).toHaveBeenCalledWith("pretty");

    onChange.mockClear();
    fireEvent.click(screen.getByRole("radio", { name: "raw" }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("holds no state of its own — the value prop alone decides what is active", () => {
    const onChange = setup("raw");

    fireEvent.click(screen.getByRole("radio", { name: "pretty" }));
    expect(onChange).toHaveBeenCalledWith("pretty");
    expect(screen.getByRole("radio", { name: "raw" })).toBeChecked();
  });

  it("roving tabindex: only the active segment is in the tab order", () => {
    setup("pretty");

    expect(screen.getByRole("radio", { name: "pretty" })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("radio", { name: "raw" })).toHaveAttribute("tabindex", "-1");
  });

  it("arrow keys move focus without selecting", () => {
    const onChange = setup("raw");
    const raw = screen.getByRole("radio", { name: "raw" });

    raw.focus();
    fireEvent.keyDown(raw, { key: "ArrowRight" });

    expect(screen.getByRole("radio", { name: "pretty" })).toHaveFocus();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("arrow focus stops at the ends — no wrap", () => {
    setup("raw");
    const raw = screen.getByRole("radio", { name: "raw" });

    raw.focus();
    fireEvent.keyDown(raw, { key: "ArrowLeft" });
    expect(raw).toHaveFocus();
  });

  it("disabled inerts the whole group but keeps the value visible", () => {
    const onChange = setup("raw", true);

    const raw = screen.getByRole("radio", { name: "raw" });
    expect(raw).toBeDisabled();
    expect(screen.getByRole("radio", { name: "pretty" })).toBeDisabled();
    expect(raw).toBeChecked();

    fireEvent.click(screen.getByRole("radio", { name: "pretty" }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("the active segment is filled, the inactive one muted — never colour alone", () => {
    setup("raw");

    expect(screen.getByRole("radio", { name: "raw" }).className).toContain("bg-canvas-soft");
    expect(screen.getByRole("radio", { name: "pretty" }).className).toContain("text-ink-mute");
  });
});
