import { createRef, useState } from "react";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { QueryBar, type QueryBarHandle } from "./QueryBar";
import type { Chip, ChipKey } from "../query";

const chip = (key: ChipKey, value: string, negated = false): Chip => ({ key, value, negated });
const HOSTS = ["api.example.com", "auth.example.com", "localhost:8080"];

function Harness({ initial = [], onChange }: { initial?: Chip[]; onChange: (chips: Chip[]) => void }) {
  const [chips, setChips] = useState<Chip[]>(initial);
  return (
    <QueryBar
      chips={chips}
      hosts={HOSTS}
      onChange={(next) => {
        onChange(next);
        setChips(next);
      }}
    />
  );
}

function mount(initial: Chip[] = []) {
  const onChange = vi.fn();
  render(<Harness initial={initial} onChange={onChange} />);
  const draft = screen.getByRole("combobox", { name: "filter" });
  act(() => draft.focus());
  const type = (text: string) => fireEvent.change(draft, { target: { value: text } });
  const press = (key: string, init: object = {}) => fireEvent.keyDown(draft, { key, ...init });
  return { draft, onChange, type, press };
}

const options = () => screen.queryAllByRole("option");
const highlighted = () => options().find((o) => o.getAttribute("aria-selected") === "true");
const chips = () => screen.queryAllByTestId("chip").map((c) => c.textContent?.replace("×", ""));

describe("typing", () => {
  it("updates the draft and its suggestions and never reports", () => {
    const { draft, onChange, type } = mount();
    for (const text of ["s", "st", "status:", "status:4"]) type(text);
    expect(draft).toHaveValue("status:4");
    expect(options().map((o) => o.textContent)).toEqual(["status:4xxclient error↵ enter"]);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("opens on focus with the six keys, the first highlighted", () => {
    mount();
    expect(options()).toHaveLength(6);
    expect(highlighted()).toHaveTextContent("host:");
    expect(highlighted()).toHaveTextContent("↵ enter");
  });
});

describe("Enter and Tab take the highlighted row", () => {
  it.each(["Enter", "Tab"])("%s puts a key into the draft, then makes the value a chip", (key) => {
    const { draft, onChange, type, press } = mount();
    type("st");
    expect(highlighted()).toHaveTextContent("status:");
    press(key);
    expect(draft).toHaveValue("status:");
    expect(onChange).not.toHaveBeenCalled();

    type("status:4");
    press(key);
    expect(onChange).toHaveBeenLastCalledWith([chip("status", "4xx")]);
    expect(draft).toHaveValue("");
    expect(chips()).toEqual(["status:4xx"]);
  });

  it("host:api takes the first host seen containing it", () => {
    const { onChange, type, press } = mount();
    type("host:api");
    press("Enter");
    expect(onChange).toHaveBeenLastCalledWith([chip("host", "api.example.com")]);
  });

  it("with no rows, Enter tries the draft as typed", () => {
    const { onChange, type, press } = mount();
    type("status:404");
    press("Escape");
    press("Enter");
    expect(onChange).toHaveBeenLastCalledWith([chip("status", "404")]);
  });

  it("Tab with an empty draft and no rows lets focus move on", () => {
    const { press } = mount();
    press("Escape");
    const tab = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    screen.getByRole("combobox").dispatchEvent(tab);
    expect(tab.defaultPrevented).toBe(false);
  });

  it("a click on a row takes it", () => {
    const { onChange, type } = mount();
    type("method:");
    fireEvent.mouseDown(screen.getByRole("option", { name: /^method: ?DELETE/ }));
    expect(onChange).toHaveBeenLastCalledWith([chip("method", "DELETE")]);
  });
});

describe("Space takes the draft as typed", () => {
  it("whatever is highlighted", () => {
    const { onChange, type, press } = mount();
    type("host:api");
    expect(highlighted()).toHaveTextContent("host:api.example.com");
    press(" ");
    expect(onChange).toHaveBeenLastCalledWith([chip("host", "api")]);
  });

  it("so two words typed straight through are two text chips", () => {
    const { onChange, type, press } = mount();
    type("login");
    press(" ");
    type("token");
    press(" ");
    expect(onChange).toHaveBeenLastCalledWith([chip("q", "login"), chip("q", "token")]);
  });

  it("but not inside quotes, where a space is part of the phrase", () => {
    const { draft, onChange, type, press } = mount();
    type('"login');
    const space = new KeyboardEvent("keydown", { key: " ", bubbles: true, cancelable: true });
    draft.dispatchEvent(space);
    expect(space.defaultPrevented).toBe(false);
    type('"login token"');
    press(" ");
    expect(onChange).toHaveBeenLastCalledWith([chip("q", "login token")]);
    expect(chips()).toEqual(['"login token"']);
  });
});

describe("↑/↓ move the highlight", () => {
  it("without wrapping, and ↑ reaches the as-typed row", () => {
    const { onChange, type, press } = mount();
    type("host:a");
    expect(options().map((o) => o.textContent?.split(/as typed|↵/)[0])).toEqual([
      "host:a",
      "host:api.example.com",
      "host:auth.example.com",
      "host:localhost:8080",
    ]);
    press("ArrowDown");
    expect(highlighted()).toHaveTextContent("host:auth.example.com");
    press("ArrowDown");
    press("ArrowDown");
    expect(highlighted()).toHaveTextContent("host:localhost:8080");
    press("ArrowUp");
    press("ArrowUp");
    press("ArrowUp");
    press("ArrowUp");
    expect(highlighted()).toHaveTextContent("host:a");
    press("Enter");
    expect(onChange).toHaveBeenLastCalledWith([chip("host", "a")]);
  });

  it("the combobox names the highlighted row", () => {
    const { draft, type, press } = mount();
    type("is:");
    press("ArrowDown");
    expect(draft).toHaveAttribute("aria-activedescendant", highlighted()?.id);
  });
});

describe("faults", () => {
  it("a draft that does not parse stays and says why, until the next keystroke", () => {
    const { draft, onChange, type, press } = mount();
    type("stauts:4xx");
    press("Enter");
    expect(screen.getByText("unknown key “stauts” — did you mean status?")).toBeInTheDocument();
    expect(draft).toHaveValue("stauts:4xx");
    expect(draft).toHaveAttribute("aria-invalid", "true");
    expect(options()).toHaveLength(0);
    expect(onChange).not.toHaveBeenCalled();

    type("status:4xx");
    expect(screen.queryByText(/unknown key/)).toBeNull();
    expect(options()).not.toHaveLength(0);
  });

  it("Space on a draft that does not parse faults too", () => {
    const { onChange, type, press } = mount();
    type("status:");
    press(" ");
    expect(screen.getByText(/status: needs a value/)).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("removing", () => {
  it("Backspace in an empty draft removes the last chip", () => {
    const { onChange, press } = mount([chip("host", "a"), chip("q", "x")]);
    press("Backspace");
    expect(onChange).toHaveBeenLastCalledWith([chip("host", "a")]);
  });

  it("Backspace in a draft with text only edits the draft", () => {
    const { onChange, type, press } = mount([chip("host", "a")]);
    type("x");
    press("Backspace");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("× removes that chip and returns focus to the draft", () => {
    const { draft, onChange } = mount([chip("host", "a"), chip("method", "GET", true), chip("q", "x")]);
    fireEvent.click(screen.getByRole("button", { name: "remove -method:GET" }));
    expect(onChange).toHaveBeenLastCalledWith([chip("host", "a"), chip("q", "x")]);
    expect(document.activeElement).toBe(draft);
  });

  it("a duplicate clears the draft and reports nothing", () => {
    const { draft, onChange, type, press } = mount([chip("host", "api")]);
    type("host:api");
    press(" ");
    expect(draft).toHaveValue("");
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("Esc", () => {
  it("closes the suggestions and keeps the draft and the chips", () => {
    const { draft, onChange, type, press } = mount([chip("host", "a")]);
    type("st");
    press("Escape");
    expect(options()).toHaveLength(0);
    expect(draft).toHaveValue("st");
    expect(chips()).toEqual(["host:a"]);
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("chips", () => {
  it("draw each kind as typed: key muted, value as sent, a method by MethodLabel, a host as wire", () => {
    mount([chip("host", "localhost:8080"), chip("method", "OPTIONS", true), chip("q", "two words")]);
    expect(chips()).toEqual(["host:localhost:8080", "-method:OPTIONS", '"two words"']);
    const [host, method] = screen.getAllByTestId("chip");
    expect(host.querySelector('[data-wire="host"]')).toHaveTextContent("localhost:8080");
    expect(within(method).getByText("OPTIONS").className).not.toBe("");
  });

  it("a chip the caller says was refused is marked with ⚠ and a warn border", () => {
    render(
      <QueryBar
        chips={[chip("host", "a"), chip("status", "4xx")]}
        hosts={[]}
        onChange={() => {}}
        refused={(c) => c.key === "status"}
      />,
    );
    const [host, status] = screen.getAllByTestId("chip");
    expect(status).toHaveAttribute("aria-invalid", "true");
    expect(status).toHaveTextContent("⚠");
    expect(status.className).toContain("border-warn");
    expect(host).not.toHaveAttribute("aria-invalid");
    expect(host.className).not.toContain("border-warn");
  });

  it("the placeholder says what to type, then invites another", () => {
    const { unmount } = render(<QueryBar chips={[]} hosts={[]} onChange={() => {}} />);
    expect(screen.getByRole("combobox")).toHaveAttribute("placeholder", "filter — status:4xx, host:api, or any text");
    unmount();
    render(<QueryBar chips={[chip("q", "x")]} hosts={[]} onChange={() => {}} />);
    expect(screen.getByRole("combobox")).toHaveAttribute("placeholder", "add a filter");
  });
});

it("the handle focuses the draft, for the screen's ⌘F", () => {
  const handle = createRef<QueryBarHandle>();
  render(<QueryBar chips={[]} hosts={[]} onChange={() => {}} handle={handle} />);
  act(() => handle.current?.focus());
  expect(document.activeElement).toBe(screen.getByRole("combobox"));
});

it("blurring closes the suggestions", () => {
  const { draft } = mount();
  expect(options()).not.toHaveLength(0);
  fireEvent.blur(draft);
  expect(options()).toHaveLength(0);
});
