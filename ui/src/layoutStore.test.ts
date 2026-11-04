import { RESET_LAYOUT_EVENT, onLayoutReset, readLayout, writeLayout, type SavedLayout } from "./layoutStore";
import wire from "./layoutWire.json";

const saved: SavedLayout = wire;

function bridge(answer: () => Promise<unknown>) {
  const calls: Array<[string, Record<string, unknown> | undefined]> = [];
  const call = <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    calls.push([command, args]);
    return answer() as Promise<T>;
  };
  return { call, calls };
}

describe("layoutStore", () => {
  it("reads the shell's saved layout through layout_read", async () => {
    const { call, calls } = bridge(() => Promise.resolve(wire));
    await expect(readLayout(call)).resolves.toEqual(saved);
    expect(calls).toEqual([["layout_read", undefined]]);
  });

  it("reads no saved layout as null", async () => {
    const { call } = bridge(() => Promise.resolve(null));
    await expect(readLayout(call)).resolves.toBeNull();
  });

  it("reads null when the shell cannot answer", async () => {
    const { call } = bridge(() => Promise.reject(new Error("no shell")));
    await expect(readLayout(call)).resolves.toBeNull();
  });

  it("writes the whole layout as layout_write's one argument", async () => {
    const { call, calls } = bridge(() => Promise.resolve(undefined));
    await writeLayout(saved, call);
    expect(calls).toEqual([["layout_write", { layout: wire }]]);
  });

  it("passes a refused write on to the caller", async () => {
    const { call } = bridge(() => Promise.reject(new Error("read-only dir")));
    await expect(writeLayout(saved, call)).rejects.toThrow("read-only dir");
  });
});

describe("onLayoutReset", () => {
  it("hears the shell's reset event and hands back its unlisten", async () => {
    const unlisten = vi.fn();
    const subscribe = vi.fn(async (_event: string, handler: () => void) => {
      handler();
      return unlisten;
    });
    const handler = vi.fn();
    (await onLayoutReset(handler, subscribe))();
    expect(subscribe).toHaveBeenCalledWith(RESET_LAYOUT_EVENT, handler);
    expect(RESET_LAYOUT_EVENT).toBe("layout-reset");
    expect(handler).toHaveBeenCalledTimes(1);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("listens to nothing, quietly, where no shell answers", async () => {
    const off = await onLayoutReset(vi.fn(), () => Promise.reject(new Error("no shell")));
    expect(off).not.toThrow();
  });
});
