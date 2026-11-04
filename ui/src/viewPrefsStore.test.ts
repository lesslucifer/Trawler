import { DEFAULT_VIEW_PREFS, readViewPrefs, writeViewPrefs, type ViewPrefs } from "./viewPrefsStore";
import wire from "./viewPrefsWire.json";

const chosen = wire as ViewPrefs;

function bridge(answer: () => Promise<unknown>) {
  const calls: Array<[string, Record<string, unknown> | undefined]> = [];
  const call = <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    calls.push([command, args]);
    return answer() as Promise<T>;
  };
  return { call, calls };
}

describe("viewPrefsStore", () => {
  it("reads the shell's saved prefs through view_prefs_read", async () => {
    const { call, calls } = bridge(() => Promise.resolve({ ...wire, query: "host:api" }));
    await expect(readViewPrefs(call)).resolves.toEqual(chosen);
    expect(calls).toEqual([["view_prefs_read", undefined]]);
  });

  it("drops each field it does not recognise and keeps the rest", async () => {
    const { call } = bridge(() => Promise.resolve({ leg: "sideways", bodyMode: "raw", lastPlatform: "palm-pilot" }));
    await expect(readViewPrefs(call)).resolves.toEqual({ bodyMode: "raw" });
  });

  it.each([
    ["no answer", () => Promise.resolve(null)],
    ["a refused read", () => Promise.reject(new Error("no shell"))],
  ])("reads nothing on %s", async (_, answer) => {
    const { call } = bridge(answer);
    await expect(readViewPrefs(call)).resolves.toEqual({});
  });

  it("writes the whole record as view_prefs_write's one argument", async () => {
    const { call, calls } = bridge(() => Promise.resolve(undefined));
    await writeViewPrefs(chosen, call);
    expect(calls).toEqual([["view_prefs_write", { prefs: wire }]]);
  });

  it("defaults to the request leg, pretty, and no remembered platform", () => {
    expect(DEFAULT_VIEW_PREFS).toEqual({ leg: "request", bodyMode: "pretty", lastPlatform: null });
  });
});
