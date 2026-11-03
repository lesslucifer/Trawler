import { act, renderHook } from "@testing-library/react";
import type { SavedLayout } from "./layoutStore";
import { DEFAULT_LAYOUT } from "./layoutBounds";
import { LAYOUT_WRITE_MS, useLayout, type LayoutStore } from "./useLayout";
import wire from "./layoutWire.json";

const saved: SavedLayout = wire;

function stubStore(read: () => Promise<SavedLayout | null> = async () => null) {
  let reset: (() => void) | null = null;
  const unlisten = vi.fn();
  const store: LayoutStore = {
    read: vi.fn(read),
    write: vi.fn(async () => {}),
    onReset: vi.fn(async (handler: () => void) => {
      reset = handler;
      return unlisten;
    }),
  };
  return { store, unlisten, fireReset: () => reset?.() };
}

async function mount(store: LayoutStore) {
  const hook = renderHook(() => useLayout(store));
  await act(async () => {});
  return hook;
}

describe("useLayout", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("restores the saved layout", async () => {
    const { store } = stubStore(async () => saved);
    const { result } = await mount(store);
    expect(result.current.layout).toEqual(saved);
  });

  it.each([
    ["no saved layout", async () => null],
    ["a refused read", () => Promise.reject(new Error("no shell"))],
  ])("starts from the defaults on %s", async (_, read) => {
    const { store } = stubStore(read as () => Promise<SavedLayout | null>);
    const { result } = await mount(store);
    expect(result.current.layout).toEqual(DEFAULT_LAYOUT);
  });

  it("clamps a stale saved size into its token bounds", async () => {
    const { store } = stubStore(async () => ({ ...saved, pane: 5000, headersShare: 0.02 }));
    const { result } = await mount(store);
    expect(result.current.layout.pane).toBe(960);
    expect(result.current.layout.headersShare).toBe(0.15);
  });

  it("keeps a size the user set before the saved layout arrived", async () => {
    let answer: (layout: SavedLayout) => void = () => {};
    const { store } = stubStore(() => new Promise((resolve) => (answer = resolve)));
    const { result } = renderHook(() => useLayout(store));
    act(() => result.current.setPane(700));
    await act(async () => answer(saved));
    expect(result.current.layout.pane).toBe(700);
  });

  it("writes nothing while sizes change, and once after the last commit", async () => {
    const { store } = stubStore();
    const { result } = await mount(store);
    act(() => {
      for (let pane = 521; pane < 600; pane++) result.current.setPane(pane);
      result.current.commit();
      result.current.setHeadersShare(0.5);
      result.current.commit();
    });
    expect(store.write).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(LAYOUT_WRITE_MS));
    expect(store.write).toHaveBeenCalledTimes(1);
    expect(store.write).toHaveBeenCalledWith({ ...DEFAULT_LAYOUT, pane: 599, headersShare: 0.5 });
  });

  it("resets one size at a time", async () => {
    const { store } = stubStore(async () => saved);
    const { result } = await mount(store);
    act(() => result.current.resetPane());
    expect(result.current.layout).toEqual({ ...saved, pane: DEFAULT_LAYOUT.pane });
    act(() => result.current.resetHeadersShare());
    expect(result.current.layout).toEqual({ ...saved, pane: 520, headersShare: 0.35 });
  });

  it("resets every size on the shell's reset event and writes it", async () => {
    const { store, fireReset } = stubStore(async () => saved);
    const { result } = await mount(store);
    act(() => fireReset());
    expect(result.current.layout).toEqual(DEFAULT_LAYOUT);
    act(() => vi.advanceTimersByTime(LAYOUT_WRITE_MS));
    expect(store.write).toHaveBeenCalledWith(DEFAULT_LAYOUT);
  });

  it("stops listening and flushes a pending write when it unmounts", async () => {
    const { store, unlisten } = stubStore();
    const { result, unmount } = await mount(store);
    act(() => {
      result.current.setPane(600);
      result.current.commit();
    });
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(store.write).toHaveBeenCalledWith({ ...DEFAULT_LAYOUT, pane: 600 });
  });

  it("stays quiet when the shell refuses a write", async () => {
    const { store } = stubStore();
    vi.mocked(store.write).mockRejectedValue(new Error("read-only dir"));
    const { result } = await mount(store);
    act(() => result.current.commit());
    await act(async () => vi.advanceTimersByTime(LAYOUT_WRITE_MS));
    expect(result.current.layout).toEqual(DEFAULT_LAYOUT);
  });

  it("saves a pane width the shell can store, whole pixels, from a fractional drag", async () => {
    const { store } = stubStore();
    const { result } = await mount(store);
    act(() => {
      result.current.setPane(600.5);
      result.current.commit();
    });
    act(() => vi.advanceTimersByTime(LAYOUT_WRITE_MS));
    expect(store.write).toHaveBeenCalledWith({ ...DEFAULT_LAYOUT, pane: 601 });
  });
});
