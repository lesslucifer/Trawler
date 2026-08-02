import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Select, type SelectOption } from "./Select";

const OPTIONS: SelectOption[] = [
  { id: "ios", label: "iOS Simulator" },
  { id: "android", label: "Android Emulator" },
  { id: "chrome", label: "Chrome" },
  { id: "firefox", label: "Firefox" },
  { id: "system", label: "System proxy" },
];

const setup = (props: Partial<Parameters<typeof Select>[0]> = {}) => {
  const onChange = vi.fn();
  const view = render(
    <Select
      options={OPTIONS}
      value={undefined}
      onChange={onChange}
      label="trust target"
      {...props}
    />,
  );
  return { onChange, view };
};

const trigger = () => screen.getByRole("button", { name: "trust target" });
const filter = () => screen.getByRole("textbox", { name: "filter options" });

describe("Select", () => {
  it("shows the placeholder in mute when nothing is selected, and no tick in the list", () => {
    setup();

    expect(trigger()).toHaveTextContent("select…");
    fireEvent.click(trigger());
    expect(screen.queryByText("✓")).not.toBeInTheDocument();
  });

  it("shows the selected option's label and ticks its row", () => {
    setup({ value: "chrome" });

    expect(trigger()).toHaveTextContent("Chrome");
    fireEvent.click(trigger());
    expect(screen.getByRole("option", { name: /Chrome/ })).toHaveAttribute("aria-selected", "true");
  });

  it("opens and closes on the trigger", () => {
    setup();

    fireEvent.click(trigger());
    expect(screen.getByRole("listbox")).toBeInTheDocument();

    fireEvent.click(trigger());
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("a click selects, closes, and returns focus to the trigger", () => {
    const { onChange } = setup();

    fireEvent.click(trigger());
    fireEvent.click(screen.getByRole("option", { name: "Firefox" }));

    expect(onChange).toHaveBeenCalledWith("firefox");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(trigger()).toHaveFocus();
  });

  it("typing filters case-insensitively on the label", () => {
    setup();
    fireEvent.click(trigger());

    fireEvent.change(filter(), { target: { value: "emul" } });

    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(screen.getByRole("option", { name: /Android Emulator/ })).toBeInTheDocument();
  });

  it("a filter that matches nothing says so, and Enter selects nothing", () => {
    const { onChange } = setup();
    fireEvent.click(trigger());

    fireEvent.change(filter(), { target: { value: "zzz" } });
    expect(screen.getByText("no match")).toBeInTheDocument();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();

    fireEvent.keyDown(filter(), { key: "Enter" });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("↑/↓ move the active row without selecting; Enter commits it", () => {
    const { onChange } = setup();

    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.keyDown(filter(), { key: "ArrowDown" });
    fireEvent.keyDown(filter(), { key: "ArrowDown" });
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.keyDown(filter(), { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith("chrome");
  });

  it("arrow movement stops at both ends — no wrap", () => {
    const { onChange } = setup();
    fireEvent.click(trigger());

    for (let i = 0; i < OPTIONS.length + 3; i += 1) {
      fireEvent.keyDown(filter(), { key: "ArrowDown" });
    }
    fireEvent.keyDown(filter(), { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith("system");

    onChange.mockClear();
    fireEvent.click(trigger());
    for (let i = 0; i < 5; i += 1) {
      fireEvent.keyDown(filter(), { key: "ArrowUp" });
    }
    fireEvent.keyDown(filter(), { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith("ios");
  });

  it("opening puts the active row on the selection, not the first row", () => {
    const { onChange } = setup({ value: "firefox" });

    fireEvent.keyDown(trigger(), { key: "Enter" });
    fireEvent.keyDown(filter(), { key: "Enter" });

    expect(onChange).toHaveBeenCalledWith("firefox");
  });

  it("Esc closes unchanged, clears the filter, and restores focus", () => {
    const { onChange } = setup();
    fireEvent.click(trigger());
    fireEvent.change(filter(), { target: { value: "chr" } });

    fireEvent.keyDown(filter(), { key: "Escape" });

    expect(onChange).not.toHaveBeenCalled();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(trigger()).toHaveFocus();

    fireEvent.click(trigger());
    expect(filter()).toHaveValue("");
  });

  it("Tab closes without selecting and never traps focus", () => {
    const { onChange } = setup();
    fireEvent.click(trigger());

    fireEvent.keyDown(filter(), { key: "Tab" });

    expect(onChange).not.toHaveBeenCalled();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("loading outranks the empty state — an empty list mid-fetch is not yet a fact", () => {
    const { onChange } = setup({ options: [], isLoading: true });
    fireEvent.click(trigger());

    expect(screen.getByText("loading…")).toBeInTheDocument();
    expect(screen.queryByText("nothing to choose from")).not.toBeInTheDocument();

    fireEvent.keyDown(trigger(), { key: "Enter" });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("a loaded empty list says nothing to choose from", () => {
    setup({ options: [] });
    fireEvent.click(trigger());

    expect(screen.getByText("nothing to choose from")).toBeInTheDocument();
  });

  it("searchable=false hides the row but not the capability — typing still filters", () => {
    setup({ searchable: false });
    fireEvent.click(trigger());

    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();

    fireEvent.keyDown(trigger(), { key: "c" });
    expect(screen.getByRole("textbox", { name: "filter options" })).toHaveValue("c");
  });

  it("an option element draws the row, and the label still feeds the filter", () => {
    setup({
      options: [{ id: "ios", label: "iOS Simulator", element: <em>iOS ⌘</em> }],
    });
    fireEvent.click(trigger());

    expect(screen.getByText("iOS ⌘").tagName).toBe("EM");

    fireEvent.change(filter(), { target: { value: "simul" } });
    expect(screen.getAllByRole("option")).toHaveLength(1);
  });

  it("disabled is out of the tab order and never opens", () => {
    setup({ disabled: true });

    expect(trigger()).toBeDisabled();
    fireEvent.click(trigger());
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("a click outside closes without selecting", () => {
    const { onChange } = setup();
    fireEvent.click(trigger());

    fireEvent.mouseDown(document.body);

    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("never returns to unselected — onChange always carries a real id", () => {
    const { onChange } = setup({ value: "chrome" });
    fireEvent.click(trigger());
    fireEvent.click(screen.getByRole("option", { name: /Chrome/ }));

    expect(onChange).toHaveBeenCalledWith("chrome");
    expect(onChange).not.toHaveBeenCalledWith(undefined);
  });

  it("the trigger holds a min-width so it never resizes with the selection", () => {
    setup();
    expect(trigger().className).toContain("min-w-[132px]");
  });

  it("never selects a row the options changed underneath — remote options arrive while open", () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <Select options={OPTIONS} value={undefined} onChange={onChange} label="trust target" />,
    );
    fireEvent.click(trigger());
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });

    const refetched: SelectOption[] = [
      { id: "edge", label: "Edge" },
      { id: "safari", label: "Safari" },
      { id: "opera", label: "Opera" },
      { id: "brave", label: "Brave" },
      { id: "vivaldi", label: "Vivaldi" },
    ];
    rerender(
      <Select options={refetched} value={undefined} onChange={onChange} label="trust target" />,
    );
    fireEvent.keyDown(trigger(), { key: "Enter" });

    expect(onChange).not.toHaveBeenCalledWith("opera");
  });

  // The costlier half of the same rule: a parent that rebuilds its options array
  // every render is the common React case, and resetting the highlight there
  // would break arrow-key navigation for every such caller.
  it("keeps the highlight when a re-render rebuilds an unchanged options array", () => {
    const onChange = vi.fn();
    const fresh = () => OPTIONS.map((o) => ({ ...o }));
    const { rerender } = render(
      <Select options={fresh()} value={undefined} onChange={onChange} label="trust target" />,
    );
    fireEvent.click(trigger());
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    rerender(
      <Select options={fresh()} value={undefined} onChange={onChange} label="trust target" />,
    );
    fireEvent.keyDown(trigger(), { key: "Enter" });

    expect(onChange).toHaveBeenCalledWith("chrome");
  });

  // The reconcile above must not fight the filter: narrowing the list resets the
  // highlight to the first match on purpose, so keying it on the filtered list
  // instead of on options silently reverts that.
  it("narrowing by filter highlights the first match, not the previously active row", () => {
    const { onChange } = setup();
    fireEvent.click(trigger());
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    fireEvent.keyDown(trigger(), { key: "e" });
    fireEvent.keyDown(trigger(), { key: "Enter" });

    expect(onChange).toHaveBeenCalledWith("android");
  });
});
