import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { CaptureMenu, type CaptureAct } from "./CaptureMenu";

const acts = (over: Partial<Record<string, Partial<CaptureAct>>> = {}): CaptureAct[] => [
  { id: "export", label: "export as har…", run: vi.fn(), ...over.export },
  { id: "import", label: "import…", name: "import a har file or a curl command", run: vi.fn(), ...over.import },
  { id: "clear", label: "clear…", separated: true, run: vi.fn(), ...over.clear },
];

const trigger = () => screen.getByRole("button", { name: "capture actions" });
const items = () => screen.getAllByRole("menuitem");
const open = () => fireEvent.click(trigger());

function mount(list: CaptureAct[] = acts()) {
  render(<CaptureMenu acts={list} label="capture actions" />);
  return list;
}

test("at rest the header carries one trigger and nothing else", () => {
  mount();
  expect(trigger()).toBeInTheDocument();
  expect(screen.queryByRole("menu")).toBeNull();
});

test("the menu names the three acts as words, in the order out, in, empty", () => {
  mount();
  open();
  expect(items().map((i) => i.textContent)).toEqual(["export as har…", "import…", "clear…"]);
});

test("only the irreversible act carries a boundary above it", () => {
  mount();
  open();
  const separated = items().filter((i) => i.className.includes("border-t"));
  expect(separated).toHaveLength(1);
  expect(separated[0]).toHaveTextContent("clear…");
});

test("an item whose label leans on the menu carries the fuller accessible name", () => {
  mount();
  open();
  expect(screen.getByRole("menuitem", { name: "import a har file or a curl command" })).toHaveTextContent(
    "import…",
  );
});

test("clicking an act runs it and closes the menu", () => {
  const list = mount();
  open();
  fireEvent.click(screen.getByText("clear…"));
  expect(list[2].run).toHaveBeenCalledOnce();
  expect(screen.queryByRole("menu")).toBeNull();
});

test("a disabled act neither runs nor takes the highlight", () => {
  const list = mount(acts({ export: { disabled: true } }));
  open();
  const item = screen.getByRole("menuitem", { name: "export as har…" });
  fireEvent.click(item);
  expect(list[0].run).not.toHaveBeenCalled();
  expect(screen.getByRole("menu")).toBeInTheDocument();
  fireEvent.mouseEnter(item);
  expect(item.className).not.toContain("bg-wash-tint");
});

test("the keyboard opens, moves without acting, and takes the moved-to item", () => {
  const list = mount();
  trigger().focus();
  fireEvent.keyDown(trigger(), { key: "Enter" });
  fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });
  fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });
  // Two moves and nothing has run — movement is not selection.
  expect(list.every((a) => (a.run as ReturnType<typeof vi.fn>).mock.calls.length === 0)).toBe(true);

  fireEvent.keyDown(screen.getByRole("menu"), { key: "Enter" });
  expect(list[2].run).toHaveBeenCalledOnce();
});

test("↑↓ stop at the ends rather than wrapping", () => {
  const list = mount();
  trigger().focus();
  fireEvent.keyDown(trigger(), { key: "ArrowDown" });
  const menu = () => screen.getByRole("menu");
  fireEvent.keyDown(menu(), { key: "ArrowUp" });
  fireEvent.keyDown(menu(), { key: "Enter" });
  expect(list[0].run).toHaveBeenCalledOnce();
});

// Movement is virtual — DOM focus stays on the trigger the whole time the menu
// is open, which is how the app's only other popup (Select) already behaves. So
// the difference between the two exits is not where focus lands but whether the
// key is consumed: Esc dismisses and keeps it, Tab is left to the browser so it
// moves on to the next control.
test.each([
  ["Escape", true],
  ["Tab", false],
])("%s closes the menu; only Esc consumes the key", (key, consumed) => {
  mount();
  trigger().focus();
  fireEvent.keyDown(trigger(), { key: "Enter" });
  const handled = !fireEvent.keyDown(screen.getByRole("menu"), { key });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(handled).toBe(consumed);
  expect(document.activeElement).toBe(trigger());
});

test("acting returns focus to the trigger", () => {
  mount();
  trigger().focus();
  fireEvent.keyDown(trigger(), { key: "Enter" });
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Enter" });
  expect(document.activeElement).toBe(trigger());
});

test("pressing the trigger again closes what it opened", () => {
  mount();
  open();
  open();
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(trigger());
});

test("clicking away dismisses it — the list underneath stays readable, so it is not modal", () => {
  mount();
  open();
  fireEvent.mouseDown(document.body);
  expect(screen.queryByRole("menu")).toBeNull();
});

test("re-opening starts at the first act rather than where the last one left off", () => {
  const list = mount();
  trigger().focus();
  fireEvent.keyDown(trigger(), { key: "Enter" });
  fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

  fireEvent.keyDown(trigger(), { key: "Enter" });
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Enter" });
  expect(list[0].run).toHaveBeenCalledOnce();
});
