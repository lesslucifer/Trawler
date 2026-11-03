import { readLayout, writeLayout, type SavedLayout } from "./layoutStore";
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
