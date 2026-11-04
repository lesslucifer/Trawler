import { act, renderHook } from "@testing-library/react";
import { DEFAULT_VIEW_PREFS, type ViewPrefs } from "./viewPrefsStore";
import { useViewPrefs, type ViewPrefsStore } from "./useViewPrefs";

function stubStore(read: () => Promise<Partial<ViewPrefs>> = async () => ({})) {
  const store: ViewPrefsStore = { read: vi.fn(read), write: vi.fn(async () => {}) };
  return store;
}

function heldRead() {
  let answer: (prefs: Partial<ViewPrefs>) => void = () => {};
  const store = stubStore(() => new Promise((resolve) => (answer = resolve)));
  return { store, answer: (prefs: Partial<ViewPrefs>) => answer(prefs) };
}

async function mount(store: ViewPrefsStore) {
  const hook = renderHook(() => useViewPrefs(store));
  await act(async () => {});
  return hook;
}

describe("useViewPrefs", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("starts from the defaults and restores what was saved", async () => {
    const { store, answer } = heldRead();
    const { result } = renderHook(() => useViewPrefs(store));
    expect(result.current.prefs).toEqual(DEFAULT_VIEW_PREFS);
    await act(async () => answer({ leg: "response", lastPlatform: "android" }));
    expect(result.current.prefs).toEqual({ leg: "response", bodyMode: "pretty", lastPlatform: "android", chips: [] });
    expect(store.read).toHaveBeenCalledTimes(1);
  });

  it("starts from the defaults when the read is refused", async () => {
    const { result } = await mount(stubStore(() => Promise.reject(new Error("no shell"))));
    expect(result.current.prefs).toEqual(DEFAULT_VIEW_PREFS);
  });

  it("keeps a choice made before the saved prefs arrived, field by field", async () => {
    const { store, answer } = heldRead();
    const { result } = renderHook(() => useViewPrefs(store));
    act(() => result.current.set("leg", "response"));
    await act(async () => answer({ leg: "request", bodyMode: "raw" }));
    expect(result.current.prefs).toEqual({ leg: "response", bodyMode: "raw", lastPlatform: null, chips: [] });
  });

  // A quit (Cmd-Q, the Dock, the last window closing) ends the process without
  // unmounting the app, so a choice still waiting on a timer would never land.
  it("sends the whole record the moment a choice is made, with no timer to outlive", async () => {
    const store = stubStore(async () => ({ lastPlatform: "ios" }));
    const { result } = await mount(store);
    act(() => result.current.set("leg", "response"));
    expect(store.write).toHaveBeenLastCalledWith({ leg: "response", bodyMode: "pretty", lastPlatform: "ios", chips: [] });
    act(() => result.current.set("bodyMode", "raw"));
    expect(store.write).toHaveBeenLastCalledWith({ leg: "response", bodyMode: "raw", lastPlatform: "ios", chips: [] });
    expect(store.write).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("writes nothing for a choice that changes nothing", async () => {
    const store = stubStore();
    const { result } = await mount(store);
    act(() => result.current.set("leg", "request"));
    expect(store.write).not.toHaveBeenCalled();
  });

  it("holds a write until the saved prefs are read, so it never writes over them", async () => {
    const { store, answer } = heldRead();
    const { result } = renderHook(() => useViewPrefs(store));
    act(() => result.current.set("leg", "response"));
    act(() => result.current.set("lastPlatform", "ios"));
    expect(store.write).not.toHaveBeenCalled();
    await act(async () => answer({ bodyMode: "raw", lastPlatform: "android" }));
    expect(store.write).toHaveBeenCalledTimes(1);
    expect(store.write).toHaveBeenCalledWith({ leg: "response", bodyMode: "raw", lastPlatform: "ios", chips: [] });
  });
});
