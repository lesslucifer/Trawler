import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import type { FlowSummary } from "./api";
import { FlowList } from "./FlowList";
import { Layout } from "./Layout";
import { DEFAULT_LAYOUT } from "./layoutBounds";
import { RESET_LAYOUT_EVENT } from "./layoutStore";
import { LAYOUT_WRITE_MS, useLayout } from "./useLayout";
import wire from "./layoutWire.json";

const bridge = vi.hoisted(() => ({
  saved: null as unknown,
  listeners: new Map<string, () => void>(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string) => (command === "layout_read" ? bridge.saved : undefined)),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: () => void) => {
    bridge.listeners.set(event, handler);
    return () => bridge.listeners.delete(event);
  }),
}));

const flows: FlowSummary[] = ["a", "b", "c"].map((id) => ({
  id,
  started_at: "2026-07-12T10:00:00Z",
  duration_ms: 42,
  method: "GET",
  uri: `http://example.com/${id}`,
  status: 200,
  source: "captured",
  seed: null,
  request_body_state: "empty",
  response_body_state: "captured",
}));

function Shell() {
  const split = useLayout();
  return (
    <Layout header={null} split={split} detail={<div>detail</div>}>
      <FlowList flows={flows} selectedId="a" split={split} />
    </Layout>
  );
}

async function launch(saved: unknown = wire) {
  bridge.saved = saved;
  render(<Shell />);
  await act(async () => {});
  return {
    host: () => screen.getByRole("separator", { name: "host column width" }),
    paneSplitter: () => screen.getByRole("separator", { name: "detail pane width" }),
    templates: () =>
      [screen.getByTestId("column-labels"), ...screen.getAllByTestId("flow-row")].map(
        (grid) => grid.style.gridTemplateColumns,
      ),
  };
}

const writes = () => vi.mocked(invoke).mock.calls.filter(([command]) => command === "layout_write");
const settle = () => act(() => vi.advanceTimersByTime(LAYOUT_WRITE_MS));
const withHost = (host: number) => ({ ...wire, columns: { ...wire.columns, host } });

describe("the user-sized columns, through the shell's layout store", () => {
  const width = window.innerWidth;
  const restore: Array<() => void> = [];

  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockClear();
    bridge.listeners.clear();
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1440 });
    for (const key of ["offsetHeight", "offsetWidth"] as const) {
      const before = Object.getOwnPropertyDescriptor(HTMLElement.prototype, key);
      Object.defineProperty(HTMLElement.prototype, key, { configurable: true, value: 600 });
      restore.push(() => before && Object.defineProperty(HTMLElement.prototype, key, before));
    }
    const rect = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockReturnValue({ height: 600, width: 600 } as DOMRect);
    restore.push(() => rect.mockRestore());
  });

  afterEach(() => {
    restore.splice(0).forEach((undo) => undo());
    vi.useRealTimers();
    Object.defineProperty(window, "innerWidth", { configurable: true, value: width });
  });

  it("opens the labels and every row at the saved widths", async () => {
    const ui = await launch();
    expect(new Set(ui.templates())).toEqual(new Set(["160px 70px 240px minmax(128px, 1fr) 70px 90px"]));
    expect(ui.host()).toHaveAttribute("aria-valuenow", "240");
  });

  it("saves a key step on a handle as one write of the whole layout", async () => {
    const ui = await launch();
    fireEvent.keyDown(ui.host(), { key: "ArrowRight" });
    expect(new Set(ui.templates())).toEqual(new Set(["160px 70px 248px minmax(128px, 1fr) 70px 90px"]));
    expect(writes()).toHaveLength(0);
    settle();
    expect(writes()).toEqual([["layout_write", { layout: withHost(248) }]]);
  });

  it("saves a column drag once, after release", async () => {
    const ui = await launch();
    fireEvent.pointerDown(ui.host(), { button: 0, clientX: 500, pointerId: 1 });
    for (let x = 501; x <= 560; x++) fireEvent.pointerMove(ui.host(), { clientX: x, pointerId: 1 });
    settle();
    expect(writes()).toHaveLength(0);
    fireEvent.pointerUp(ui.host(), { pointerId: 1 });
    settle();
    expect(writes()).toEqual([["layout_write", { layout: withHost(300) }]]);
  });

  it("lowers splitter A's live maximum as a column widens, without touching the saved pane", async () => {
    const ui = await launch();
    expect(ui.paneSplitter()).toHaveAttribute("aria-valuemax", "589");
    fireEvent.keyDown(ui.host(), { key: "End" });
    expect(ui.paneSplitter()).toHaveAttribute("aria-valuemax", "384");
    expect(screen.getByTestId("detail-pane")).toHaveStyle({ width: "384px" });
    settle();
    expect(writes()).toEqual([["layout_write", { layout: withHost(480) }]]);
  });

  it("resets one column on a double-click and every column on the shell's Reset Layout", async () => {
    const ui = await launch({ ...withHost(300), columns: { ...withHost(300).columns, method: 100 } });
    fireEvent.doubleClick(ui.host());
    expect(ui.host()).toHaveAttribute("aria-valuenow", "180");
    expect(screen.getByRole("separator", { name: "method column width" })).toHaveAttribute("aria-valuenow", "100");
    act(() => bridge.listeners.get(RESET_LAYOUT_EVENT)?.());
    expect(new Set(ui.templates())).toEqual(new Set(["160px 70px 180px minmax(128px, 1fr) 70px 90px"]));
    settle();
    expect(writes()).toEqual([["layout_write", { layout: DEFAULT_LAYOUT }]]);
  });
});
