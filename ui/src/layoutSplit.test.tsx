import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import type { FlowView } from "./api";
import { apiStub } from "./apiStub";
import { FlowDetail } from "./FlowDetail";
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

const flow: FlowView = {
  id: "f1",
  started_at: "2026-07-12T10:00:00Z",
  duration_ms: 42,
  method: "GET",
  uri: "http://example.com/a",
  status: 200,
  source: "captured",
  seed: null,
  request_http_version: "HTTP/1.1",
  response_http_version: "HTTP/1.1",
  request_headers: [{ name: "Accept", value: "*/*" }],
  response_headers: [],
  request_body: { state: "empty" },
  response_body: { state: "empty" },
};

function Shell() {
  const split = useLayout();
  return (
    <Layout
      header={null}
      split={split}
      detail={<FlowDetail split={split} api={apiStub()} flow={flow} onClose={() => {}} />}
    >
      rows
    </Layout>
  );
}

function setWindowWidth(width: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width });
  act(() => {
    window.dispatchEvent(new Event("resize"));
  });
}

async function launch(saved: unknown = wire) {
  bridge.saved = saved;
  render(<Shell />);
  await act(async () => {});
  return {
    pane: () => screen.getByTestId("detail-pane"),
    paneSplitter: () => screen.getByRole("separator", { name: "detail pane width" }),
    headersSplitter: () => screen.getByRole("separator", { name: "headers height" }),
  };
}

const writes = () => vi.mocked(invoke).mock.calls.filter(([command]) => command === "layout_write");
const settle = () => act(() => vi.advanceTimersByTime(LAYOUT_WRITE_MS));

describe("the user-sized splits, through the shell's layout store", () => {
  const width = window.innerWidth;
  let tall: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockClear();
    bridge.listeners.clear();
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1600 });
    tall = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ height: 400 } as DOMRect);
  });

  afterEach(() => {
    tall.mockRestore();
    vi.useRealTimers();
    Object.defineProperty(window, "innerWidth", { configurable: true, value: width });
  });

  it("opens the pane and the headers at the saved sizes", async () => {
    const ui = await launch();
    expect(ui.pane()).toHaveStyle({ width: "600px" });
    expect(screen.getByTestId("headers-zone")).toHaveStyle({ maxHeight: "42%" });
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("layout_read");
  });

  it("opens at the defaults with nothing saved", async () => {
    const ui = await launch(null);
    expect(ui.pane()).toHaveStyle({ width: "520px" });
    expect(screen.getByTestId("headers-zone")).toHaveStyle({ maxHeight: "35%" });
  });

  it("saves a key step on each splitter as one write of the whole layout", async () => {
    const ui = await launch();
    fireEvent.keyDown(ui.paneSplitter(), { key: "ArrowLeft" });
    fireEvent.keyDown(ui.headersSplitter(), { key: "ArrowDown" });
    expect(ui.pane()).toHaveStyle({ width: "608px" });
    expect(writes()).toHaveLength(0);
    settle();
    expect(writes()).toHaveLength(1);
    const [[, args]] = writes() as unknown as [[string, { layout: typeof wire }]];
    expect(args.layout).toEqual({ ...wire, pane: 608, headersShare: expect.closeTo(0.44) });
  });

  it("saves a drag once, after release", async () => {
    const ui = await launch();
    fireEvent.pointerDown(ui.paneSplitter(), { button: 0, clientX: 1000, pointerId: 1 });
    for (let x = 999; x >= 900; x--) fireEvent.pointerMove(ui.paneSplitter(), { clientX: x, pointerId: 1 });
    settle();
    expect(writes()).toHaveLength(0);
    fireEvent.pointerUp(ui.paneSplitter(), { pointerId: 1 });
    settle();
    expect(writes()).toEqual([["layout_write", { layout: { ...wire, pane: 700 } }]]);
  });

  it("saves a drag under a scaled display as whole pixels", async () => {
    const ui = await launch();
    fireEvent.pointerDown(ui.paneSplitter(), { button: 0, clientX: 1000, pointerId: 1 });
    fireEvent.pointerMove(ui.paneSplitter(), { clientX: 949.5, pointerId: 1 });
    fireEvent.pointerUp(ui.paneSplitter(), { pointerId: 1 });
    settle();
    const [[, args]] = writes() as unknown as [[string, { layout: typeof wire }]];
    expect(Number.isInteger(args.layout.pane)).toBe(true);
    expect(args.layout.pane).toBe(651);
  });

  it("puts every size back on the shell's Reset Layout", async () => {
    const ui = await launch();
    act(() => bridge.listeners.get(RESET_LAYOUT_EVENT)?.());
    expect(ui.pane()).toHaveStyle({ width: "520px" });
    expect(screen.getByTestId("headers-zone")).toHaveStyle({ maxHeight: "35%" });
    settle();
    expect(writes()).toEqual([["layout_write", { layout: DEFAULT_LAYOUT }]]);
  });

  it("clamps the pane to what a narrower window leaves the list, and shows the saved width again when it grows", async () => {
    const ui = await launch();
    setWindowWidth(1311);
    expect(ui.pane()).toHaveStyle({ width: "460px" });
    expect(ui.paneSplitter()).toHaveAttribute("aria-valuemax", "460");
    setWindowWidth(850);
    expect(ui.pane()).toHaveStyle({ width: "384px" });
    setWindowWidth(1600);
    expect(ui.pane()).toHaveStyle({ width: "600px" });
    settle();
    expect(writes()).toHaveLength(0);
  });
});
