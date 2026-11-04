import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { IconButton } from "./IconButton";

describe("IconButton", () => {
  it("draws a named glyph from the registry and names itself after it", () => {
    const onClick = vi.fn();
    render(<IconButton glyph="close" onClick={onClick} />);

    const button = screen.getByRole("button", { name: "close" });
    expect(button).toHaveTextContent("×");
    expect(button).toHaveAttribute("title", "close");

    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("save is the registry's other entry", () => {
    render(<IconButton glyph="save" onClick={() => {}} />);
    expect(screen.getByRole("button", { name: "save" })).toHaveTextContent("↓");
  });

  it.each([
    ["more", "⋯"],
    ["window", "↗"],
    ["add", "+"],
  ] as const)("%s draws %s and is named after it unless labelled", (glyph, face) => {
    render(<IconButton glyph={glyph} onClick={() => {}} />);
    expect(screen.getByRole("button", { name: glyph })).toHaveTextContent(face);
  });

  it("a custom glyph wins over the named one, and label carries the accessible name", () => {
    render(
      <IconButton glyph="close" label="collapse panel" onClick={() => {}}>
        ⌄
      </IconButton>,
    );

    const button = screen.getByRole("button", { name: "collapse panel" });
    expect(button).toHaveTextContent("⌄");
    expect(button).not.toHaveTextContent("×");
  });

  it("passes the three axes onto the shell: bare, square, 28px, mute", () => {
    render(<IconButton glyph="close" onClick={() => {}} />);

    const button = screen.getByRole("button");
    expect(button).toHaveStyle({ width: "28px", height: "28px" });
    expect(button.className).toContain("border-none");
    expect(button.className).toContain("text-ink-mute");
    expect(button.className).toContain("hover:bg-wash-hover");
  });

  it("size overrides but never drops below the 24px floor", () => {
    const { rerender } = render(<IconButton glyph="close" onClick={() => {}} size={32} />);
    expect(screen.getByRole("button")).toHaveStyle({ width: "32px" });

    rerender(<IconButton glyph="close" onClick={() => {}} size={16} />);
    expect(screen.getByRole("button")).toHaveStyle({ width: "24px" });
  });

  it("disabled takes no click and inherits the shell's treatment", () => {
    const onClick = vi.fn();
    render(<IconButton glyph="close" onClick={onClick} disabled />);

    const button = screen.getByRole("button");
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
    expect(button.className).toContain("disabled:opacity-disabled");
  });

  it("help is a ? in a circle that forwards whether its region is shown", () => {
    render(<IconButton glyph="help" label="hide notes" expanded controls="note" onClick={() => {}} />);

    const button = screen.getByRole("button", { name: "hide notes" });
    expect(button).toHaveTextContent("?");
    expect(button.querySelector("svg[data-glyph='help'] circle")).not.toBeNull();
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(button).toHaveAttribute("aria-controls", "note");
  });

  it("its @avoid alternative exists: a word-labelled action is a Button", async () => {
    const { Button } = await import("./Button");
    expect(Button).toBeTypeOf("function");
  });
});
