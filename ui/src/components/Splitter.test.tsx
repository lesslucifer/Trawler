import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { Splitter } from "./Splitter";

type Props = Partial<Parameters<typeof Splitter>[0]>;

function setup(props: Props = {}) {
  const onChange = vi.fn();
  const onCommit = vi.fn();
  const onReset = vi.fn();
  const view = render(
    <Splitter
      axis="x"
      value={520}
      min={384}
      max={960}
      dir={-1}
      onChange={onChange}
      onCommit={onCommit}
      onReset={onReset}
      label="detail pane width"
      valueText="520 px"
      {...props}
    />,
  );
  return { onChange, onCommit, onReset, view, handle: screen.getByRole("separator") };
}

function Live({ initial = 520 }: { initial?: number }) {
  const [value, setValue] = useState(initial);
  return (
    <Splitter
      axis="x"
      value={value}
      min={384}
      max={960}
      dir={-1}
      onChange={setValue}
      onCommit={() => {}}
      onReset={() => setValue(initial)}
      label="detail pane width"
      valueText={`${value} px`}
    />
  );
}

describe("Splitter", () => {
  it("is a focusable separator naming its value and bounds", () => {
    const { handle } = setup();
    expect(handle).toHaveAttribute("tabindex", "0");
    expect(handle).toHaveAttribute("aria-orientation", "vertical");
    expect(handle).toHaveAttribute("aria-valuenow", "520");
    expect(handle).toHaveAttribute("aria-valuemin", "384");
    expect(handle).toHaveAttribute("aria-valuemax", "960");
    expect(handle).toHaveAttribute("aria-valuetext", "520 px");
    expect(handle).toHaveAccessibleName("detail pane width");
  });

  it("lies horizontally between zones stacked on the y axis", () => {
    const { handle } = setup({ axis: "y", dir: 1 });
    expect(handle).toHaveAttribute("aria-orientation", "horizontal");
    expect(handle.className).toContain("cursor-row-resize");
  });

  it("moves the line where the arrow points, 8 px a step and 48 with Shift, then commits", () => {
    const { handle, onChange, onCommit } = setup();
    fireEvent.keyDown(handle, { key: "ArrowLeft" });
    expect(onChange).toHaveBeenLastCalledWith(528);
    fireEvent.keyDown(handle, { key: "ArrowRight", shiftKey: true });
    expect(onChange).toHaveBeenLastCalledWith(472);
    expect(onCommit).toHaveBeenCalledTimes(2);
  });

  it("steps a value that grows along the axis with dir 1, in the caller's unit", () => {
    const { handle, onChange } = setup({ axis: "y", dir: 1, value: 0.35, min: 0.15, max: 0.6, perPx: 1 / 400 });
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    expect(onChange).toHaveBeenLastCalledWith(0.37);
    fireEvent.keyDown(handle, { key: "ArrowUp", shiftKey: true });
    expect(onChange.mock.lastCall?.[0]).toBeCloseTo(0.23);
  });

  it("ignores arrows across its axis and Enter", () => {
    const { handle, onChange, onCommit, onReset } = setup();
    for (const key of ["ArrowUp", "ArrowDown", "Enter", " "]) fireEvent.keyDown(handle, { key });
    expect(onChange).not.toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();
    expect(onReset).not.toHaveBeenCalled();
  });

  it("jumps to its bounds with Home and End", () => {
    const { handle, onChange } = setup();
    fireEvent.keyDown(handle, { key: "Home" });
    expect(onChange).toHaveBeenLastCalledWith(384);
    fireEvent.keyDown(handle, { key: "End" });
    expect(onChange).toHaveBeenLastCalledWith(960);
  });

  it("never steps past a bound", () => {
    const { handle, onChange } = setup({ value: 380 + 8 });
    fireEvent.keyDown(handle, { key: "ArrowRight", shiftKey: true });
    expect(onChange).toHaveBeenLastCalledWith(384);
  });

  it("resets on a double-click and commits the reset", () => {
    const { handle, onReset, onCommit } = setup();
    fireEvent.doubleClick(handle);
    expect(onReset).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("follows a drag start-relative, clamped, and commits once on release", () => {
    const { handle, onChange, onCommit } = setup();
    fireEvent.pointerDown(handle, { button: 0, clientX: 900, pointerId: 1 });
    for (let x = 899; x >= 600; x--) fireEvent.pointerMove(handle, { clientX: x, pointerId: 1 });
    expect(onChange).toHaveBeenCalledTimes(300);
    expect(onChange).toHaveBeenLastCalledWith(820);
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.pointerMove(handle, { clientX: 0, pointerId: 1 });
    expect(onChange).toHaveBeenLastCalledWith(960);
    fireEvent.pointerMove(handle, { clientX: 880, pointerId: 1 });
    expect(onChange).toHaveBeenLastCalledWith(540);
    fireEvent.pointerUp(handle, { pointerId: 1 });
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("commits nothing for a press that never moved", () => {
    const { handle, onCommit } = setup();
    fireEvent.pointerDown(handle, { button: 0, clientX: 900, pointerId: 1 });
    fireEvent.pointerUp(handle, { pointerId: 1 });
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("ignores moves without a press and presses of another button", () => {
    const { handle, onChange } = setup();
    fireEvent.pointerMove(handle, { clientX: 10 });
    fireEvent.pointerDown(handle, { button: 2, clientX: 900 });
    fireEvent.pointerMove(handle, { clientX: 10 });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("shows the whole document the resize cursor and no text selection only while dragging", () => {
    const { handle } = setup();
    const root = document.documentElement.classList;
    fireEvent.pointerDown(handle, { button: 0, clientX: 900, pointerId: 1 });
    expect(root.contains("cursor-col-resize")).toBe(true);
    expect(root.contains("select-none")).toBe(true);
    fireEvent.pointerCancel(handle, { pointerId: 1 });
    expect(root.contains("cursor-col-resize")).toBe(false);
    expect(root.contains("select-none")).toBe(false);
  });

  it("lifts the line while dragging", () => {
    const { handle } = setup();
    const line = screen.getByTestId("splitter-line");
    expect(line.className).toContain("bg-hairline");
    fireEvent.pointerDown(handle, { button: 0, clientX: 900, pointerId: 1 });
    expect(line.className).toContain("bg-ink-mute");
    expect(line.className).not.toContain("bg-hairline");
  });

  it("shows a value above a lowered max at the max, without reporting a change", () => {
    const { handle, onChange, view } = setup();
    view.rerender(
      <Splitter
        axis="x"
        value={520}
        min={384}
        max={489}
        dir={-1}
        onChange={onChange}
        onCommit={() => {}}
        onReset={() => {}}
        label="detail pane width"
        valueText="489 px"
      />,
    );
    expect(handle).toHaveAttribute("aria-valuenow", "489");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("takes the caller's band placement in place of its own", () => {
    const { handle } = setup({ className: "inset-y-0 -right-3 w-3" });
    expect(handle.className).toContain("-right-3");
    expect(handle.className).not.toContain("-inset-x-grip-reach");
  });

  it("drives a caller's state from the keyboard and back to its default", () => {
    render(<Live />);
    const handle = screen.getByRole("separator");
    fireEvent.keyDown(handle, { key: "End" });
    expect(handle).toHaveAttribute("aria-valuenow", "960");
    fireEvent.doubleClick(handle);
    expect(handle).toHaveAttribute("aria-valuenow", "520");
  });
});
