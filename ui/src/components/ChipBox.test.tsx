import { createRef, useState } from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ChipBox, type ChipBoxHandle, type ChipGrammar } from "./ChipBox";

interface Word {
  key?: string;
  text: string;
}

const grammar: ChipGrammar<Word> = {
  parse: (draft) => {
    const text = draft.trim();
    if (!text) return { empty: true };
    if (text === "bad") return { fault: "bad is not a word" };
    const keyed = /^(\w+):(.+)$/.exec(text);
    return { chip: keyed ? { key: keyed[1], text: keyed[2] } : { text } };
  },
  suggest: (draft) =>
    draft === "k" ? [{ kind: "key", text: "key:", hint: "a key" }] : draft === "key:a" ? [{ kind: "value", text: "key:alpha", hint: "" }] : [],
  label: (word) => ({ text: word.key ? `${word.key}:${word.text}` : word.text, key: word.key, value: word.text }),
  hasOpenQuote: (draft) => (draft.match(/"/g) ?? []).length % 2 === 1,
};

interface Spies {
  onDraft?: (draft: string) => void;
  onEnter?: (shift: boolean) => void;
  onEscape?: () => void;
  onChange?: (chips: Word[]) => void;
  refused?: (word: Word) => string | null;
  start?: Word[];
}

function mount(spies: Spies = {}) {
  const handle = createRef<ChipBoxHandle>();
  function Harness() {
    const [chips, setChips] = useState<Word[]>(spies.start ?? []);
    return (
      <ChipBox
        chips={chips}
        onChange={(next) => {
          spies.onChange?.(next);
          setChips(next);
        }}
        grammar={grammar}
        refused={spies.refused}
        label="box"
        placeholder="type"
        chippedPlaceholder="add"
        onDraft={spies.onDraft}
        onEnter={spies.onEnter}
        onEscape={spies.onEscape}
        handle={handle}
      />
    );
  }
  render(<Harness />);
  const draft = screen.getByRole("combobox", { name: "box" });
  act(() => draft.focus());
  return {
    draft,
    handle,
    type: (text: string) => fireEvent.change(draft, { target: { value: text } }),
    press: (key: string, init: object = {}) => fireEvent.keyDown(draft, { key, ...init }),
  };
}

const chipTexts = () => screen.queryAllByTestId("chip").map((c) => c.textContent?.replace("×", ""));

describe("ChipBox", () => {
  it("Space makes a chip from a draft that parses and shows the grammar's fault otherwise", () => {
    const { draft, type, press } = mount();
    type("one");
    press(" ");
    expect(chipTexts()).toEqual(["one"]);
    expect(draft).toHaveValue("");
    expect(draft).toHaveAttribute("placeholder", "add");
    type("bad");
    press(" ");
    expect(screen.getByText("bad is not a word")).toBeInTheDocument();
    expect(chipTexts()).toEqual(["one"]);
    type("ba");
    expect(screen.queryByText("bad is not a word")).toBeNull();
  });

  it("a key goes into the draft and a value becomes a chip", () => {
    const { draft, type, press } = mount();
    type("k");
    expect(screen.getAllByRole("option")).toHaveLength(1);
    press("Tab");
    expect(draft).toHaveValue("key:");
    type("key:a");
    press("Enter");
    expect(chipTexts()).toEqual(["key:alpha"]);
  });

  it("Space inside an open quote stays in the draft; a repeated chip is not added twice", () => {
    const { draft, type, press } = mount();
    type('"a');
    press(" ");
    expect(draft).toHaveValue('"a');
    type("x");
    press(" ");
    press(" ");
    type("x");
    press(" ");
    expect(chipTexts()).toEqual(["x"]);
  });

  it("Backspace in an empty draft removes the last chip; a chip's × removes that one", () => {
    const onChange = vi.fn();
    const { press } = mount({ start: [{ text: "a" }, { text: "b" }, { text: "c" }], onChange });
    press("Backspace");
    expect(chipTexts()).toEqual(["a", "b"]);
    fireEvent.click(screen.getByRole("button", { name: "remove a" }));
    expect(chipTexts()).toEqual(["b"]);
    expect(onChange).toHaveBeenCalledTimes(2);
  });

  it("onDraft reports every keystroke and Enter with nothing highlighted calls onEnter instead of making a chip", () => {
    const onDraft = vi.fn();
    const onEnter = vi.fn();
    const { type, press } = mount({ onDraft, onEnter });
    type("w");
    type("wo");
    expect(onDraft.mock.calls.map(([d]) => d)).toEqual(["w", "wo"]);
    press("Enter");
    press("Enter", { shiftKey: true });
    expect(onEnter.mock.calls).toEqual([[false], [true]]);
    expect(chipTexts()).toEqual([]);
  });

  it("Enter takes a highlighted suggestion even when onEnter is given", () => {
    const onEnter = vi.fn();
    const { draft, type, press } = mount({ onEnter });
    type("k");
    press("Enter");
    expect(draft).toHaveValue("key:");
    expect(onEnter).not.toHaveBeenCalled();
  });

  it("Esc closes the suggestions first, then clears the draft and calls onEscape", () => {
    const onEscape = vi.fn();
    const { draft, type, press } = mount({ onEscape });
    type("k");
    press("Escape");
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(draft).toHaveValue("k");
    expect(onEscape).not.toHaveBeenCalled();
    press("Escape");
    expect(draft).toHaveValue("");
    expect(onEscape).toHaveBeenCalledTimes(1);
  });

  it("without onEscape, Enter takes the draft and Esc only closes", () => {
    const { draft, type, press } = mount();
    type("plain");
    press("Enter");
    expect(chipTexts()).toEqual(["plain"]);
    type("k");
    press("Escape");
    expect(draft).toHaveValue("k");
  });

  it("a refused chip wears the warn border and ⚠ and its reason as the title", () => {
    mount({ start: [{ key: "key", text: "id" }, { text: "ok" }], refused: (w) => (w.key ? "key needs JSON" : null) });
    const [refused, fine] = screen.getAllByTestId("chip");
    expect(refused).toHaveAttribute("aria-invalid", "true");
    expect(refused).toHaveAttribute("title", "key needs JSON");
    expect(refused).toHaveClass("border-warn");
    expect(refused).toHaveTextContent("⚠");
    expect(fine).not.toHaveAttribute("aria-invalid");
    expect(fine).not.toHaveAttribute("title");
  });

  it("starts from a given draft and focuses through its handle", () => {
    const handle = createRef<ChipBoxHandle>();
    render(<ChipBox chips={[]} onChange={() => {}} grammar={grammar} label="seeded" placeholder="p" initialDraft="seed" handle={handle} />);
    const input = screen.getByRole("combobox", { name: "seeded" });
    expect(input).toHaveValue("seed");
    act(() => handle.current?.focus());
    expect(input).toHaveFocus();
  });
});
