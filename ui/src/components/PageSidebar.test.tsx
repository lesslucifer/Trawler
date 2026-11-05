import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { PageSidebar } from "./PageSidebar";

const PAGES = [
  { value: "general", label: "General" },
  { value: "service", label: "Capture service" },
] as const;

const setup = (value = "general") => {
  const onChange = vi.fn();
  render(<PageSidebar pages={PAGES} value={value} onChange={onChange} ariaLabel="Settings pages" />);
  return onChange;
};

describe("PageSidebar", () => {
  it("is a vertical tablist with exactly one selected page", () => {
    setup("general");

    const list = screen.getByRole("tablist", { name: "Settings pages" });
    expect(list).toHaveAttribute("aria-orientation", "vertical");
    expect(screen.getByRole("tab", { name: "General" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Capture service" })).toHaveAttribute("aria-selected", "false");
  });

  it("clicking another page reports it; the current page is a no-op", () => {
    const onChange = setup("general");

    fireEvent.click(screen.getByRole("tab", { name: "Capture service" }));
    expect(onChange).toHaveBeenCalledWith("service");

    onChange.mockClear();
    fireEvent.click(screen.getByRole("tab", { name: "General" }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("holds no state of its own — the value prop alone decides the current page", () => {
    setup("general");

    fireEvent.click(screen.getByRole("tab", { name: "Capture service" }));
    expect(screen.getByRole("tab", { name: "General" })).toHaveAttribute("aria-selected", "true");
  });

  it("roving tabindex: only the current page is in the tab order", () => {
    setup("service");

    expect(screen.getByRole("tab", { name: "Capture service" })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("tab", { name: "General" })).toHaveAttribute("tabindex", "-1");
  });

  it("↓ and ↑ move to the next or previous page and show it, without wrapping", () => {
    const onChange = setup("general");
    const general = screen.getByRole("tab", { name: "General" });
    const service = screen.getByRole("tab", { name: "Capture service" });

    fireEvent.keyDown(general, { key: "ArrowDown" });
    expect(onChange).toHaveBeenLastCalledWith("service");
    expect(service).toHaveFocus();

    onChange.mockClear();
    fireEvent.keyDown(service, { key: "ArrowDown" });
    fireEvent.keyDown(general, { key: "ArrowUp" });
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.keyDown(service, { key: "ArrowUp" });
    expect(onChange).toHaveBeenLastCalledWith("general");
    expect(general).toHaveFocus();
  });

  it("marks the current page in ink and the others ink-mute", () => {
    setup("general");

    expect(screen.getByRole("tab", { name: "General" })).toHaveClass("text-ink");
    expect(screen.getByRole("tab", { name: "Capture service" })).toHaveClass("text-ink-mute");
  });
});
