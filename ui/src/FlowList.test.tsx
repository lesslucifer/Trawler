import { render, screen, fireEvent } from "@testing-library/react";
import { vi } from "vitest";
import { FlowList } from "./FlowList";
import { defaultProxyAddr, type FlowSummary } from "./api";

function flow(over: Partial<FlowSummary> = {}): FlowSummary {
  return {
    id: "f1",
    started_at: new Date().toISOString(),
    duration_ms: 42,
    method: "GET",
    uri: "http://example.com/a/b?x=1",
    status: 200,
    request_body_state: "empty",
    response_body_state: "captured",
    ...over,
  };
}

const three = [
  flow({ id: "a" }),
  flow({ id: "b", method: "DELETE", status: 403 }),
  flow({ id: "c", method: "POST", status: null, duration_ms: null }),
];

// jsdom reports 0 layout, so react-virtual would window down to no rows at all.
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    value: 600,
  });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
    configurable: true,
    value: 900,
  });
  HTMLElement.prototype.getBoundingClientRect = () =>
    ({
      height: 600,
      width: 900,
      top: 0,
      left: 0,
      bottom: 600,
      right: 900,
      x: 0,
      y: 0,
      toJSON() {},
    }) as DOMRect;
});

const rows = () => screen.getAllByTestId("flow-row");
const listbox = () => screen.getByRole("listbox");

test("each row cell renders through the component that owns its value", () => {
  render(<FlowList flows={[flow()]} />);
  const row = rows()[0];

  expect(row).toHaveTextContent(/\d{2}:\d{2}:\d{2}/);
  expect(screen.getByText("GET").className).toMatch(/text-ok/);
  expect(screen.getByLabelText("200 OK")).toBeInTheDocument();
  expect(row).toHaveTextContent("42 ms");
});

test("a pending flow holds its duration column instead of collapsing it", () => {
  render(<FlowList flows={[flow({ status: null, duration_ms: null })]} />);

  expect(screen.getByText("…")).toBeInTheDocument();
  const placeholder = rows()[0].querySelector("[aria-hidden='true']");
  expect(placeholder?.className).toMatch(/text-transparent/);
});

test("the verb is toned by what the request does to the resource", () => {
  render(<FlowList flows={three} />);

  expect(screen.getByText("DELETE").className).toMatch(/text-warn/);
  expect(screen.getByText("POST").className).toMatch(/text-create/);
});

// The design records host/path truncation as a narrow, deliberate violation of
// wrap-not-truncate that a fixed-height virtualized row cannot avoid — and the
// row's title is what pays for it. Removing either half breaks the bargain.
test("host and path truncate, and the row carries the full path as its title", () => {
  render(<FlowList flows={[flow()]} />);

  expect(screen.getByText("example.com").className).toMatch(/truncate/);
  expect(screen.getByText("/a/b?x=1").className).toMatch(/truncate/);
  expect(rows()[0]).toHaveAttribute("title", "/a/b?x=1");
});

test("an empty list with no filter names the act that fills it", () => {
  render(<FlowList flows={[]} />);

  expect(screen.getByTestId("empty")).toHaveTextContent("no flows yet");
  expect(screen.getByTestId("empty")).toHaveTextContent(defaultProxyAddr());
  expect(screen.getByTestId("empty")).not.toHaveTextContent(
    "no flows match this filter",
  );
});

test("an empty list with a filter says the query excludes the traffic, not that none exists", () => {
  render(<FlowList flows={[]} filtered />);

  expect(screen.getByTestId("empty")).toHaveTextContent(
    "no flows match this filter",
  );
  expect(screen.getByTestId("empty")).toHaveTextContent(
    "clear a field to widen the search",
  );
  expect(screen.getByTestId("empty")).not.toHaveTextContent("no flows yet");
});

test("the row region is one tab stop whose options carry the selection", () => {
  render(<FlowList flows={three} selectedId="b" />);

  expect(listbox()).toHaveAttribute("tabindex", "0");
  expect(rows().map((r) => r.getAttribute("aria-selected"))).toEqual([
    "false",
    "true",
    "false",
  ]);
  expect(listbox()).toHaveAttribute("aria-activedescendant", "flow-row-b");
});

test("arrow keys move the selection one row at a time", () => {
  const onSelect = vi.fn();
  render(<FlowList flows={three} selectedId="a" onSelect={onSelect} />);

  fireEvent.keyDown(listbox(), { key: "ArrowDown" });
  expect(onSelect).toHaveBeenCalledWith("b");

  onSelect.mockClear();
  render(<FlowList flows={three} selectedId="b" onSelect={onSelect} />);
  fireEvent.keyDown(screen.getAllByRole("listbox")[1], { key: "ArrowUp" });
  expect(onSelect).toHaveBeenCalledWith("a");
});

test("the selection does not wrap at either end", () => {
  const onSelect = vi.fn();
  const { unmount } = render(
    <FlowList flows={three} selectedId="a" onSelect={onSelect} />,
  );
  fireEvent.keyDown(listbox(), { key: "ArrowUp" });
  expect(onSelect).not.toHaveBeenCalled();
  unmount();

  render(<FlowList flows={three} selectedId="c" onSelect={onSelect} />);
  fireEvent.keyDown(listbox(), { key: "ArrowDown" });
  expect(onSelect).not.toHaveBeenCalled();
});

test("arrowing into an unselected list starts at the first row", () => {
  const onSelect = vi.fn();
  render(<FlowList flows={three} onSelect={onSelect} />);

  fireEvent.keyDown(listbox(), { key: "ArrowDown" });
  expect(onSelect).toHaveBeenCalledWith("a");
});

test("Enter and Space open the selected row", () => {
  const onSelect = vi.fn();
  const { unmount } = render(
    <FlowList flows={three} selectedId="b" onSelect={onSelect} />,
  );
  fireEvent.keyDown(listbox(), { key: "Enter" });
  expect(onSelect).toHaveBeenCalledWith("b");
  unmount();

  onSelect.mockClear();
  render(<FlowList flows={three} selectedId="b" onSelect={onSelect} />);
  fireEvent.keyDown(listbox(), { key: " " });
  expect(onSelect).toHaveBeenCalledWith("b");
});

// A 200-row list is windowed down to a fraction of itself, so a selection driven
// past that window has no element to scroll to unless the component asks the
// virtualizer for one. jsdom implements no scrollTo, so the virtualizer's own
// scroll is a no-op here and the window cannot actually follow — what this
// asserts is that the request is made for every step, including the ones past
// the window's edge. That the browser then honours it is the e2e's assertion.
test("the keyboard asks the virtualizer to follow a selection past the rendered window", () => {
  const scrollTo = vi.fn();
  HTMLElement.prototype.scrollTo = scrollTo;
  const many = Array.from({ length: 200 }, (_, i) => flow({ id: `f${i}` }));
  const onSelect = vi.fn();
  const { rerender } = render(<FlowList flows={many} selectedId="f0" onSelect={onSelect} />);

  const rendered = () => rows().map((r) => r.id);
  expect(rendered().length).toBeLessThan(many.length);
  const ids = rendered();
  const lastRendered = Number(ids[ids.length - 1].replace("flow-row-f", ""));

  let selected = "f0";
  for (let i = 0; i <= lastRendered; i++) {
    scrollTo.mockClear();
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    selected = onSelect.mock.lastCall![0] as string;
    expect(scrollTo).toHaveBeenCalled();
    rerender(<FlowList flows={many} selectedId={selected} onSelect={onSelect} />);
  }

  // The last step selected a row the window had never rendered.
  expect(Number(selected.replace("f", ""))).toBeGreaterThan(lastRendered);
});

test("arrowing up into an unselected list also starts at the first row", () => {
  const onSelect = vi.fn();
  render(<FlowList flows={three} onSelect={onSelect} />);

  fireEvent.keyDown(listbox(), { key: "ArrowUp" });
  expect(onSelect).toHaveBeenCalledWith("a");
});

test("clicking a row still selects it", () => {
  const onSelect = vi.fn();
  render(<FlowList flows={three} onSelect={onSelect} />);

  fireEvent.click(rows()[1]);
  expect(onSelect).toHaveBeenCalledWith("b");
});

test("the status column is centred with its label rather than sharing the duration's edge", () => {
  render(<FlowList flows={[flow()]} />);

  expect(screen.getByText("Status").className).toMatch(/text-center/);
  expect(screen.getByLabelText("200 OK").parentElement?.className).toMatch(
    /text-center/,
  );
});
