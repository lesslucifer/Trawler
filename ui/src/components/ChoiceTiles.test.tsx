import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { ChoiceTiles } from "./ChoiceTiles";

const OPTIONS = [
  { id: "this", label: "This machine — macOS" },
  { id: "ios", label: "iPhone or iPad" },
  { id: "android", label: "Android device" },
] as const;

const setup = (value = "this", disabled = false) => {
  const onChange = vi.fn();
  render(
    <ChoiceTiles
      options={OPTIONS}
      value={value}
      onChange={onChange}
      ariaLabel="which device's traffic do you want to see"
      disabled={disabled}
    />,
  );
  return onChange;
};

describe("ChoiceTiles", () => {
  it("renders every option — the whole point is that none is hidden", () => {
    setup();

    expect(screen.getAllByRole("radio")).toHaveLength(OPTIONS.length);
    for (const option of OPTIONS) {
      expect(screen.getByRole("radio", { name: option.label })).toBeInTheDocument();
    }
  });

  it("is a radiogroup carrying the caller's question, with exactly one taken", () => {
    setup("ios");

    expect(
      screen.getByRole("radiogroup", { name: "which device's traffic do you want to see" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "iPhone or iPad" })).toBeChecked();
    expect(screen.getAllByRole("radio").filter((tile) => tile.getAttribute("aria-checked") === "true"))
      .toHaveLength(1);
  });

  it("clicking another tile reports its id; the taken one is a no-op", () => {
    const onChange = setup("this");

    fireEvent.click(screen.getByRole("radio", { name: "iPhone or iPad" }));
    expect(onChange).toHaveBeenCalledWith("ios");

    onChange.mockClear();
    fireEvent.click(screen.getByRole("radio", { name: "This machine — macOS" }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("holds no state of its own — the value prop alone decides what is taken", () => {
    setup("this");

    fireEvent.click(screen.getByRole("radio", { name: "Android device" }));

    expect(screen.getByRole("radio", { name: "This machine — macOS" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Android device" })).not.toBeChecked();
  });

  it("roving tabindex: the group is one tab stop, and it is the taken tile", () => {
    setup("ios");

    expect(screen.getByRole("radio", { name: "iPhone or iPad" })).toHaveAttribute("tabindex", "0");
    expect(
      screen.getAllByRole("radio").filter((tile) => tile.getAttribute("tabindex") === "0"),
    ).toHaveLength(1);
  });

  it.each([
    ["ArrowRight", "iPhone or iPad"],
    ["ArrowDown", "iPhone or iPad"],
  ])("%s moves focus to the next tile without committing", (key, landing) => {
    const onChange = setup("this");
    const first = screen.getByRole("radio", { name: "This machine — macOS" });

    first.focus();
    fireEvent.keyDown(first, { key });

    expect(screen.getByRole("radio", { name: landing })).toHaveFocus();
    expect(onChange).not.toHaveBeenCalled();
    expect(first).toBeChecked();
  });

  it.each(["ArrowLeft", "ArrowUp"])("%s moves focus back, and the row does not wrap", (key) => {
    setup("this");
    const first = screen.getByRole("radio", { name: "This machine — macOS" });

    first.focus();
    fireEvent.keyDown(first, { key });

    expect(first).toHaveFocus();
  });

  it("the focused tile is taken by the activation the browser already gives a button", () => {
    const onChange = setup("this");
    const android = screen.getByRole("radio", { name: "Android device" });

    android.focus();
    fireEvent.click(android);

    expect(onChange).toHaveBeenCalledWith("android");
  });

  it("disabled inerts the whole group and keeps the answer visible", () => {
    const onChange = setup("ios", true);

    for (const tile of screen.getAllByRole("radio")) expect(tile).toBeDisabled();
    expect(screen.getByRole("radio", { name: "iPhone or iPad" })).toBeChecked();

    fireEvent.click(screen.getByRole("radio", { name: "Android device" }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("the taken tile carries ink and its border, the rest stay mute — never colour alone", () => {
    setup("this");

    expect(screen.getByRole("radio", { name: "This machine — macOS" }).className).toContain(
      "border-ink-mute",
    );
    expect(screen.getByRole("radio", { name: "Android device" }).className).toContain("text-ink-mute");
  });

  it("the row wraps rather than scrolling — a long label lengthens the column", () => {
    setup();

    const group = screen.getByRole("radiogroup");
    expect(group.className).toContain("flex-wrap");
    expect(group.className).not.toContain("overflow");
  });
});
