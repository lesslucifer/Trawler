import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { useRef, useState } from "react";
import { Menu, type MenuItem } from "./Menu";

function sections(over: { undo?: () => void; sort?: () => void; pick?: (id: string) => void } = {}): MenuItem[][] {
  return [
    [
      { label: "undo", keys: "⌘Z", run: over.undo ?? (() => {}) },
      { label: "redo", keys: "⇧⌘Z", disabled: true, why: "nothing to redo", run: () => {} },
      { label: "restore original", run: () => {} },
    ],
    [
      { label: "indent", choices: [{ id: "2", label: "2" }, { id: "4", label: "4" }, { id: "tab", label: "tab" }], value: "2", pick: over.pick ?? (() => {}) },
      { label: "sort keys", run: over.sort ?? (() => {}) },
      { label: "wrap long lines", keys: "⌥Z", checked: true, run: () => {} },
    ],
  ];
}

const menu = () => screen.getByRole("menu");
const key = (k: string, init: KeyboardEventInit = {}) => fireEvent.keyDown(menu(), { key: k, ...init });
const activeLabel = () => document.querySelector(".bg-wash-tint")?.textContent;

describe("rows", () => {
  it("each section is split by a rule, each row shows its key, a disabled one its reason", () => {
    render(<Menu label="more" sections={sections()} at={{ x: 10, y: 10 }} onClose={() => {}} />);
    expect(menu()).toHaveAccessibleName("more");
    expect(screen.getByRole("menuitem", { name: /undo/ })).toHaveTextContent("⌘Z");
    const redo = screen.getByRole("menuitem", { name: /redo/ });
    expect(redo).toHaveAttribute("aria-disabled", "true");
    expect(redo).toHaveTextContent("nothing to redo");
    expect(redo).not.toHaveTextContent("⇧⌘Z");
    expect(menu().querySelectorAll(".border-t")).toHaveLength(1);
  });

  it("a check row carries a tick, a choice row marks the chosen one", () => {
    render(<Menu label="more" sections={sections()} at={{ x: 0, y: 0 }} onClose={() => {}} />);
    expect(screen.getByRole("menuitemcheckbox", { name: /wrap long lines/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("menuitemcheckbox")).toHaveTextContent("✓");
    expect(screen.getByRole("menuitemradio", { name: "2" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("menuitemradio", { name: "2" }).className).toContain("bg-wash-active");
  });
});

describe("keyboard", () => {
  it("focus moves into the menu on the first live row; ↑ ↓ skip disabled rows and wrap", () => {
    render(<Menu label="more" sections={sections()} at={{ x: 0, y: 0 }} onClose={() => {}} />);
    expect(menu()).toHaveFocus();
    expect(activeLabel()).toContain("undo");
    key("ArrowDown");
    expect(activeLabel()).toContain("restore original");
    key("ArrowUp");
    key("ArrowUp");
    expect(activeLabel()).toContain("wrap long lines");
    key("Home");
    expect(activeLabel()).toContain("undo");
    key("End");
    expect(activeLabel()).toContain("wrap long lines");
  });

  it("a typed letter moves to the next label starting with it", () => {
    render(<Menu label="more" sections={sections()} at={{ x: 0, y: 0 }} onClose={() => {}} />);
    key("s");
    expect(activeLabel()).toContain("sort keys");
    key("r");
    expect(activeLabel()).toContain("restore original");
  });

  it("Enter runs the active action and closes", () => {
    const sort = vi.fn();
    const onClose = vi.fn();
    render(<Menu label="more" sections={sections({ sort })} at={{ x: 0, y: 0 }} onClose={onClose} />);
    key("s");
    key("Enter");
    expect(sort).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("← → on a choice row pick the next choice and leave the menu open", () => {
    const pick = vi.fn();
    const onClose = vi.fn();
    render(<Menu label="more" sections={sections({ pick })} at={{ x: 0, y: 0 }} onClose={onClose} />);
    key("i");
    key("ArrowRight");
    key("ArrowLeft");
    expect(pick.mock.calls).toEqual([["4"], ["tab"]]);
    key("Enter");
    expect(onClose).not.toHaveBeenCalled();
  });

  it("with every row disabled the keys move nothing, and Esc still closes", () => {
    const onClose = vi.fn();
    render(
      <Menu label="selection" sections={[[{ label: "copy", disabled: true, why: "select some text", run: () => {} }]]} at={{ x: 0, y: 0 }} onClose={onClose} />,
    );
    key("ArrowDown");
    key("End");
    key("c");
    key("Enter");
    expect(document.querySelector(".bg-wash-tint")).toBeNull();
    key("Escape");
    expect(onClose).toHaveBeenCalledOnce();
  });
});

describe("pointer", () => {
  it("hover makes a row active; a click runs it and closes; a disabled row does nothing", () => {
    const undo = vi.fn();
    const onClose = vi.fn();
    render(<Menu label="more" sections={sections({ undo })} at={{ x: 0, y: 0 }} onClose={onClose} />);
    fireEvent.mouseEnter(screen.getByRole("menuitem", { name: /restore original/ }));
    expect(activeLabel()).toContain("restore original");
    fireEvent.click(screen.getByRole("menuitem", { name: /redo/ }));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("menuitem", { name: /undo/ }));
    expect(undo).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("a click on a choice picks it and stays open", () => {
    const pick = vi.fn();
    const onClose = vi.fn();
    render(<Menu label="more" sections={sections({ pick })} at={{ x: 0, y: 0 }} onClose={onClose} />);
    fireEvent.click(screen.getByRole("menuitemradio", { name: "tab" }));
    expect(pick).toHaveBeenCalledWith("tab");
    expect(onClose).not.toHaveBeenCalled();
  });
});

function Anchored({ onClose }: { onClose: () => void }) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(true);
  return (
    <>
      <button ref={trigger} type="button" onClick={() => setOpen((o) => !o)}>
        ⋯
      </button>
      <button type="button">elsewhere</button>
      {open && (
        <Menu
          label="more"
          sections={sections()}
          anchor={trigger}
          onClose={() => {
            onClose();
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

describe("dismissal", () => {
  it("Esc closes and focus returns to the trigger", () => {
    const onClose = vi.fn();
    render(<Anchored onClose={onClose} />);
    key("Escape");
    expect(onClose).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "⋯" })).toHaveFocus();
  });

  it("a press outside closes it; a press on the trigger is the trigger's to toggle", () => {
    const onClose = vi.fn();
    render(<Anchored onClose={onClose} />);
    fireEvent.mouseDown(screen.getByRole("button", { name: "⋯" }));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.mouseDown(screen.getByRole("button", { name: "elsewhere" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("the window losing focus closes it", () => {
    const onClose = vi.fn();
    render(<Menu label="more" sections={sections()} at={{ x: 0, y: 0 }} onClose={onClose} />);
    fireEvent.blur(window);
    expect(onClose).toHaveBeenCalledOnce();
  });
});

describe("placement", () => {
  it("at a point it opens there, moved in to stay inside the window", () => {
    render(<Menu label="more" sections={sections()} at={{ x: 40, y: 30 }} onClose={() => {}} />);
    expect(menu()).toHaveStyle({ left: "40px", top: "30px", visibility: "visible" });
  });

  it("a point past the window's edge is pulled back inside it", () => {
    render(<Menu label="more" sections={sections()} at={{ x: window.innerWidth + 50, y: window.innerHeight + 50 }} onClose={() => {}} />);
    expect(menu()).toHaveStyle({ left: `${window.innerWidth - 8}px`, top: `${window.innerHeight - 8}px` });
  });
});
