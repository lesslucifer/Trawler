import { render, screen, fireEvent, within } from "@testing-library/react";
import { vi } from "vitest";
import { FlowList } from "./FlowList";
import { DEFAULT_COLUMNS, DEFAULT_LAYOUT, ROW_CHROME, columnTemplate, listMin } from "./layoutBounds";
import type { LayoutControls } from "./useLayout";
import type { FlowSummary } from "./api";

function flow(over: Partial<FlowSummary> = {}): FlowSummary {
  return {
    id: "f1",
    started_at: new Date().toISOString(),
    duration_ms: 42,
    method: "GET",
    uri: "http://example.com/a/b?x=1",
    status: 200,
    source: "captured",
    seed: null,
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

  expect(screen.getByTestId("empty")).toHaveTextContent("Nothing captured yet");
  expect(screen.getByTestId("empty")).toHaveTextContent("once they go through Trawler");
  expect(screen.getByTestId("empty")).not.toHaveTextContent(
    "Nothing matches your filter",
  );
});

// Every empty is the whole of the row region, so each is stated centred in it.
test("every empty is centred in the region, as the region's own statement", () => {
  const cases = [{}, { filtered: true }, { cleared: true }, { refused: "abc" }, { silent: true }];
  for (const props of cases) {
    const { unmount } = render(<FlowList flows={[]} {...props} />);
    const region = screen.getByTestId("empty");
    expect(region.className).toContain("items-center");
    expect(region.className).toContain("justify-center");
    expect(region.querySelector("[data-layout]")).toHaveAttribute("data-layout", "region");
    unmount();
  }
});

// The owner, on round 10: less technical, more user friendly. No empty names an
// address, a port or a piece of Trawler's insides.
test("no empty speaks in addresses or internals", () => {
  const cases = [{}, { filtered: true }, { cleared: true }, { refused: "abc" }, { silent: true }];
  for (const props of cases) {
    const { unmount } = render(<FlowList flows={[]} onSetUp={() => {}} {...props} />);
    const said = screen.getByTestId("empty").textContent ?? "";
    expect(said).not.toMatch(/\d+\.\d+\.\d+\.\d+|:\d{4}|\bcore\b|\bproxy\b|\bflows?\b|\bclass\b/i);
    unmount();
  }
});

// P4: a window nothing has reached is the one empty whose remedy is setup.
test("an empty list with nothing captured offers set up capture", () => {
  const onSetUp = vi.fn();
  render(<FlowList flows={[]} onSetUp={onSetUp} />);

  fireEvent.click(within(screen.getByTestId("empty")).getByRole("button", { name: "set up capture" }));
  expect(onSetUp).toHaveBeenCalledOnce();
});

test("a filtered, cleared, refused or silent empty offers no setup: none of them is a window nothing reached", () => {
  const cases = [{ filtered: true }, { cleared: true }, { refused: "abc" }, { silent: true }];
  for (const props of cases) {
    const { unmount } = render(<FlowList flows={[]} onSetUp={() => {}} {...props} />);
    expect(screen.queryByRole("button", { name: "set up capture" })).toBeNull();
    unmount();
  }
});

test("an empty list with a filter says the query excludes the traffic, not that none exists", () => {
  render(<FlowList flows={[]} filtered />);

  expect(screen.getByTestId("empty")).toHaveTextContent(
    "Nothing matches your filter",
  );
  expect(screen.getByTestId("empty")).toHaveTextContent(
    "Try a shorter search, or clear one of the fields above.",
  );
  expect(screen.getByTestId("empty")).not.toHaveTextContent("Nothing captured yet");
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

test("an imported row carries the mark, with its own text channel — colour is never the only one", () => {
  render(<FlowList flows={[flow({ id: "i", source: "imported" })]} />);
  const mark = screen.getByLabelText("imported");
  expect(mark).toHaveTextContent("↑");
  // Ink, not chromatic: being imported is a provenance, not an outcome.
  expect(mark.className).toContain("text-ink-mute");
});

test("a captured row still holds the mark's box, so every timestamp shares one left edge", () => {
  const { container } = render(<FlowList flows={[flow({ id: "c", source: "captured" })]} />);
  const cell = container.querySelector('[data-wire="started_at"]');
  const reserve = cell?.firstElementChild;
  expect(screen.queryByLabelText("imported")).toBeNull();
  expect(reserve).toHaveTextContent("");
  expect(reserve?.className).toContain("w-4");
});

test("cell 1 is sized by the cross-day timestamp an import produces, not by the mark", () => {
  const { container } = render(<FlowList flows={[flow()]} />);
  const row = container.querySelector('[data-testid="flow-row"]') as HTMLElement;
  expect(row.style.gridTemplateColumns.split(" ")[0]).toBe("160px");
});

test("a cleared list gets its own heading — telling someone to send traffic they just sent is the bug", () => {
  render(<FlowList flows={[]} cleared />);
  const absence = screen.getByTestId("empty");
  expect(absence).toHaveTextContent("All cleared");
  expect(absence).not.toHaveTextContent("Nothing captured yet");
  // Same remedy as the never-captured absence, because after a clear it is.
  expect(absence).toHaveTextContent("New requests will show up here as they arrive.");
});

test("the three absences never share a sentence", () => {
  const heading = (props: { filtered?: boolean; cleared?: boolean }) => {
    const { unmount } = render(<FlowList flows={[]} {...props} />);
    const text = screen.getByTestId("empty").textContent ?? "";
    unmount();
    return text;
  };
  const wordings = [heading({}), heading({ filtered: true }), heading({ cleared: true })];
  expect(new Set(wordings).size).toBe(3);
});

test("a clear that emptied a filtered view says it was cleared, not that the filter excludes everything", () => {
  render(<FlowList flows={[]} filtered cleared />);
  expect(screen.getByTestId("empty")).toHaveTextContent("All cleared");
});

// Five causes, five sentences, and an order between them. The order is the point:
// an empty list means nothing when the list could not be fetched, so a failure
// outranks both empties rather than sharing a region with them.
test("a core that is not answering takes the empty region from both empties", () => {
  for (const filtered of [false, true]) {
    const { unmount } = render(
      <FlowList flows={[]} filtered={filtered} silent />,
    );
    const region = screen.getByTestId("empty");
    expect(region).toHaveTextContent("Trawler's capture service isn't answering");
    expect(within(region).getByRole("button", { name: "try again" })).toBeEnabled();
    expect(region).not.toHaveTextContent("Nothing captured yet");
    expect(region).not.toHaveTextContent("Nothing matches your filter");
    unmount();
  }
});

// The trailing clause explains why THIS region is empty, so it is only said when
// that is what emptied it. A filter that excludes everything is not a window
// nothing has reached, and claiming so would be the shell inventing history.
test("the silence says nothing reached the window only when nothing did", () => {
  const { unmount } = render(<FlowList flows={[]} silent />);
  expect(screen.getByTestId("empty")).toHaveTextContent("Nothing has been captured yet.");
  unmount();

  render(<FlowList flows={[]} filtered silent />);
  expect(screen.getByTestId("empty")).not.toHaveTextContent("Nothing has been captured yet.");
  expect(screen.getByTestId("empty")).toHaveTextContent("Trawler's capture service isn't answering");
});

// The same reasoning that keeps the clause off a filtered region keeps it off a
// cleared one: the traffic arrived, the user emptied it, and saying nothing has
// reached the window would be the shell inventing a history it can see is wrong.
test("a cleared capture that then loses its core is not told nothing ever arrived", () => {
  render(<FlowList flows={[]} silent cleared />);
  const region = screen.getByTestId("empty");
  expect(region).toHaveTextContent("Trawler's capture service isn't answering");
  expect(region).not.toHaveTextContent("Nothing has been captured yet.");
});

// The run ranked "not answering" above the two empties and never against a
// refusal, so the build does not either: the refusal keeps the region it earned
// and the silence is stated above it by the shell.
test("a refusal is not hidden by the silence that followed it", () => {
  render(<FlowList flows={[]} silent refused="4xz" />);
  const region = screen.getByTestId("empty");
  expect(region).toHaveTextContent("“4xz” isn't a status code");
  expect(region).not.toHaveTextContent("Trawler's capture service isn't answering");
});

test("the empty region's retry is offered, and reports the act", () => {
  const onRetry = vi.fn();
  render(<FlowList flows={[]} silent onRetry={onRetry} />);
  fireEvent.click(screen.getByRole("button", { name: "try again" }));
  expect(onRetry).toHaveBeenCalledOnce();
});

// Fork D4: disabled and a progress line are one obligation, never half of one.
test("a retry in flight disables its button and says what is awaited", () => {
  render(<FlowList flows={[]} silent retrying />);
  expect(screen.getByRole("button", { name: "try again" })).toBeDisabled();
  expect(screen.getByTestId("empty")).toHaveTextContent("Trying again…");
  expect(screen.getByTestId("empty")).not.toHaveTextContent("the rows below");
});

// A rejection is not a failure: the API answered and said no, so there is nothing
// to retry — the correction is more typing, and the sentence names what was typed.
test("a refused status names the value and offers a grammar, not a retry", () => {
  render(<FlowList flows={[]} filtered refused="4xz" />);
  const region = screen.getByTestId("empty");
  expect(region).toHaveTextContent("“4xz” isn't a status code");
  expect(region).toHaveTextContent("Try a code like 404, or a range like 4xx.");
  expect(region).not.toHaveTextContent("Nothing matches your filter");
  expect(screen.queryByRole("button", { name: "try again" })).toBeNull();
});

test("with no failure the two inherited empties are unchanged", () => {
  const { unmount } = render(<FlowList flows={[]} silent={false} refused={null} />);
  expect(screen.getByTestId("empty")).toHaveTextContent("Nothing captured yet");
  unmount();

  render(<FlowList flows={[]} filtered silent={false} refused={null} />);
  expect(screen.getByTestId("empty")).toHaveTextContent("Nothing matches your filter");
});

// The empty region only exists when the region is empty, which is what keeps the
// statement from being said twice when the shell's band is also up.
test("rows on screen leave the empty region absent whatever the failure", () => {
  render(<FlowList flows={three} silent />);
  expect(screen.queryByTestId("empty")).toBeNull();
  expect(screen.getAllByTestId("flow-row")).toHaveLength(3);});

test("a sent row carries ↗ in the same reserved box, named sent", () => {
  const { container } = render(<FlowList flows={[flow({ id: "s", source: "sent", seed: "c" })]} />);
  const mark = screen.getByLabelText("sent");
  expect(mark).toHaveTextContent("↗");
  expect(mark.className).toContain("text-ink-mute");
  expect(mark.className).toContain("w-4");
  expect(container.querySelector('[data-wire="started_at"]')?.firstElementChild).toBe(mark);
  expect(screen.queryByLabelText("imported")).toBeNull();
});

test("each provenance wears its own face, and a captured row wears none", () => {
  render(
    <FlowList
      flows={[flow({ id: "c", source: "captured" }), flow({ id: "i", source: "imported" }), flow({ id: "s", source: "sent" })]}
    />,
  );
  const faces = screen.getAllByTestId("flow-row").map((row) => row.querySelector('[data-wire="started_at"]')?.firstElementChild?.textContent);
  expect(faces).toEqual(["", "↑", "↗"]);
});

function controls(columns = DEFAULT_COLUMNS): LayoutControls {
  return {
    layout: { ...DEFAULT_LAYOUT, columns },
    setPane: vi.fn(),
    setHeadersShare: vi.fn(),
    resetPane: vi.fn(),
    resetHeadersShare: vi.fn(),
    setColumn: vi.fn(),
    resetColumn: vi.fn(),
    commit: vi.fn(),
  };
}

const handle = (name: string) => screen.getByRole("separator", { name: `${name} column width` });

test("the label row and every row lay out the saved column widths, with the chrome the pane's bound counts", () => {
  const { container } = render(
    <FlowList flows={[flow()]} split={controls({ time: 200, method: 70, host: 300, status: 70, duration: 90 })} />,
  );
  const row = container.querySelector('[data-testid="flow-row"]') as HTMLElement;
  const label = screen.getByTestId("column-labels");
  for (const grid of [row, label]) {
    expect(grid.style.gridTemplateColumns).toBe("200px 70px 300px minmax(128px, 1fr) 70px 90px");
    expect(grid.classList).toContain(ROW_CHROME.inset);
    expect(grid.classList).toContain(ROW_CHROME.gap);
  }
});

test("the label row scrolls sideways with the rows, at least as wide as the row's minimum", () => {
  const columns = { ...DEFAULT_COLUMNS, host: 480 };
  render(<FlowList flows={[flow()]} split={controls(columns)} />);
  const scroller = screen.getByTestId("flow-columns");
  expect(scroller.className).toMatch(/overflow-x-auto/);
  expect(scroller).toContainElement(screen.getByTestId("column-labels"));
  expect(scroller).toContainElement(listbox());
  expect(screen.getByTestId("column-labels").parentElement).toHaveStyle({ minWidth: `${listMin(columns)}px` });
  expect(listbox().className).toMatch(/overflow-y-auto/);
});

test("each fixed column carries a handle bounded by its own tokens; path carries none", () => {
  render(<FlowList flows={[flow()]} split={controls()} />);
  const bounds = [
    ["time", 160, 136, 240],
    ["method", 70, 56, 112],
    ["host", 180, 96, 480],
    ["status", 70, 48, 112],
    ["duration", 90, 64, 160],
  ] as const;
  expect(screen.getAllByRole("separator")).toHaveLength(bounds.length);
  for (const [name, now, min, max] of bounds) {
    expect(handle(name)).toHaveAttribute("aria-valuenow", String(now));
    expect(handle(name)).toHaveAttribute("aria-valuemin", String(min));
    expect(handle(name)).toHaveAttribute("aria-valuemax", String(max));
    expect(handle(name)).toHaveAttribute("aria-valuetext", `${now} px`);
  }
});

test("a handle before the path widens its column rightwards, one after it leftwards", () => {
  const split = controls();
  render(<FlowList flows={[flow()]} split={split} />);
  fireEvent.keyDown(handle("host"), { key: "ArrowRight" });
  fireEvent.keyDown(handle("status"), { key: "ArrowLeft" });
  fireEvent.keyDown(handle("duration"), { key: "ArrowLeft", shiftKey: true });
  expect(split.setColumn).toHaveBeenNthCalledWith(1, "host", 188);
  expect(split.setColumn).toHaveBeenNthCalledWith(2, "status", 78);
  expect(split.setColumn).toHaveBeenNthCalledWith(3, "duration", 138);
  expect(split.commit).toHaveBeenCalledTimes(3);
});

test("a column drag reports the width clamped to the track's bounds and commits once on release", () => {
  const split = controls();
  render(<FlowList flows={[flow()]} split={split} />);
  const host = handle("host");
  fireEvent.pointerDown(host, { button: 0, clientX: 400, pointerId: 1 });
  fireEvent.pointerMove(host, { clientX: 1000, pointerId: 1 });
  fireEvent.pointerMove(host, { clientX: 0, pointerId: 1 });
  fireEvent.pointerUp(host, { pointerId: 1 });
  expect(vi.mocked(split.setColumn).mock.calls).toEqual([
    ["host", 480],
    ["host", 96],
  ]);
  expect(split.commit).toHaveBeenCalledTimes(1);
});

test("a double-click resets that one column", () => {
  const split = controls({ ...DEFAULT_COLUMNS, method: 100 });
  render(<FlowList flows={[flow()]} split={split} />);
  fireEvent.doubleClick(handle("method"));
  expect(split.resetColumn).toHaveBeenCalledWith("method");
  expect(split.commit).toHaveBeenCalledTimes(1);
});

test("the column handles come before the rows in tab order", () => {
  render(<FlowList flows={three} split={controls()} />);
  const focusable = Array.from(document.querySelectorAll<HTMLElement>('[tabindex="0"]'));
  expect(focusable.map((el) => el.getAttribute("role"))).toEqual([...Array(5).fill("separator"), "listbox"]);
});

test("the handles are drawn on an empty list too, so columns size before traffic", () => {
  render(<FlowList flows={[]} split={controls()} />);
  expect(screen.getAllByRole("separator")).toHaveLength(5);
});

test("a list with no layout to size draws the default columns and no handles", () => {
  const { container } = render(<FlowList flows={[flow()]} />);
  const row = container.querySelector('[data-testid="flow-row"]') as HTMLElement;
  expect(row.style.gridTemplateColumns).toBe(columnTemplate(DEFAULT_COLUMNS));
  expect(screen.queryByRole("separator")).toBeNull();
});
