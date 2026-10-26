import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { bounded, NO_QUIT_FACTS, promptFor, QUIT_READ_MS, useProxyCloseConfirm, type QuitFacts } from "./closeConfirm";

const ADDR = "127.0.0.1:7420";
const FACTS: QuitFacts = { answering: true, ownAddresses: [], reachArmed: false, emulator: null };

// A fake Tauri window + invoke: captures the onCloseRequested handler so a test can
// fire a close, stubs proxy_status / notice_dismissed per case, and records
// which commands + destroy() ran.
function fakeDeps(status: "ours" | "foreign" | "none" | "unknown", dismissed: string[] = []) {
  let closeHandler: (e: { preventDefault: () => void }) => unknown = () => {};
  const destroy = vi.fn(async () => {});
  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "proxy_status") return { status, address: null };
    if (cmd === "notice_dismissed") return dismissed.includes(args?.key as string);
    return undefined;
  });
  const win = {
    onCloseRequested: async (h: (e: { preventDefault: () => void }) => void) => {
      closeHandler = h;
      return () => {};
    },
    destroy,
  };
  return { deps: { getWindow: () => win, invoke: invoke as never }, destroy, invoke, fireClose: () => closeHandler };
}

function confirmFor(f: ReturnType<typeof fakeDeps>, facts: Partial<QuitFacts> = {}) {
  const read = async () => ({ ...FACTS, ...facts });
  return renderHook(() => useProxyCloseConfirm(ADDR, read, f.deps));
}

async function closeAndWait(f: ReturnType<typeof fakeDeps>) {
  const preventDefault = vi.fn();
  await act(async () => {
    await f.fireClose()({ preventDefault });
  });
  return preventDefault;
}

describe("which quits ask, and what", () => {
  test("this machine routed through Trawler asks, saying whether the capture service answers", () => {
    expect(promptFor("ours", FACTS)).toEqual({ mode: "ours", answering: true });
    expect(promptFor("ours", { ...FACTS, answering: false })).toEqual({ mode: "ours", answering: false });
  });

  test("an unread proxy asks, and never reads as no proxy (D2)", () => {
    expect(promptFor("unknown", FACTS)).toEqual({ mode: "unread" });
  });

  test("a proxy that is not Trawler's asks nothing of its own (D3)", () => {
    expect(promptFor("foreign", FACTS)).toBeNull();
  });

  test("with nothing routed here, only a device that could route here asks (D4)", () => {
    expect(promptFor("none", FACTS)).toBeNull();
    expect(promptFor("none", { ...FACTS, reachArmed: true })).toEqual({ mode: "devices", emulator: null });
    expect(promptFor("foreign", { ...FACTS, emulator: "Pixel_7_API_35" })).toEqual({
      mode: "devices",
      emulator: "Pixel_7_API_35",
    });
  });
});

test("ours: prevents the quit, opens the prompt, and asks with this machine's own addresses", async () => {
  const f = fakeDeps("ours");
  const { result } = confirmFor(f, { ownAddresses: ["192.168.1.24"] });
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).toHaveBeenCalledOnce();
  await waitFor(() => expect(result.current.prompt).toEqual({ mode: "ours", answering: true }));
  expect(f.invoke).toHaveBeenCalledWith("proxy_status", {
    host: "127.0.0.1",
    port: 7420,
    own: ["192.168.1.24"],
  });
});

test("ours + stop routing: reverts the proxy then quits", async () => {
  const f = fakeDeps("ours");
  const { result } = confirmFor(f);
  await closeAndWait(f);
  await act(async () => {
    await result.current.stop();
  });
  expect(f.invoke).toHaveBeenCalledWith("unset_proxy");
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("ours + quit: leaves the proxy up, quits, and persists nothing", async () => {
  const f = fakeDeps("ours");
  const { result } = confirmFor(f);
  await closeAndWait(f);
  await act(async () => {
    await result.current.quit(true);
  });
  expect(f.invoke).not.toHaveBeenCalledWith("unset_proxy");
  expect(f.invoke).not.toHaveBeenCalledWith("dismiss_notice", expect.anything());
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("foreign, nothing else: quits without asking", async () => {
  const f = fakeDeps("foreign");
  const { result } = confirmFor(f);
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).not.toHaveBeenCalled();
  expect(result.current.prompt).toBeNull();
});

test("none with reach armed: the devices prompt", async () => {
  const f = fakeDeps("none");
  const { result } = confirmFor(f, { reachArmed: true });
  await closeAndWait(f);
  await waitFor(() => expect(result.current.prompt).toEqual({ mode: "devices", emulator: null }));
});

test("unknown: its own prompt, unless its own opt-out was taken", async () => {
  const asked = fakeDeps("unknown");
  const { result } = confirmFor(asked);
  await closeAndWait(asked);
  await waitFor(() => expect(result.current.prompt).toEqual({ mode: "unread" }));

  const dismissed = fakeDeps("unknown", ["unread-notice"]);
  const silent = confirmFor(dismissed);
  expect(await closeAndWait(dismissed)).not.toHaveBeenCalled();
  expect(silent.result.current.prompt).toBeNull();
});

test("devices + its opt-out already taken: quits without asking", async () => {
  const f = fakeDeps("none", ["remote-notice"]);
  const { result } = confirmFor(f, { reachArmed: true });
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).not.toHaveBeenCalled();
  expect(result.current.prompt).toBeNull();
});

test.each([
  ["none", { reachArmed: true }, "remote-notice"],
  ["unknown", {}, "unread-notice"],
] as const)("%s + don't show again: persists its own key, then quits", async (status, facts, key) => {
  const f = fakeDeps(status);
  const { result } = confirmFor(f, facts);
  await closeAndWait(f);
  await waitFor(() => expect(result.current.prompt).not.toBeNull());
  await act(async () => {
    await result.current.quit(true);
  });
  expect(f.invoke).toHaveBeenCalledWith("dismiss_notice", { key });
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("devices + quit without ticking: quits, persists nothing", async () => {
  const f = fakeDeps("none");
  const { result } = confirmFor(f, { reachArmed: true });
  await closeAndWait(f);
  await act(async () => {
    await result.current.quit(false);
  });
  expect(f.invoke).not.toHaveBeenCalledWith("dismiss_notice", expect.anything());
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("a shell that cannot read the proxy, or facts that fail, still settle the close (D2)", async () => {
  const f = fakeDeps("ours");
  f.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "proxy_status") throw new Error("no such command");
    return false;
  });
  const { result } = renderHook(() =>
    useProxyCloseConfirm(ADDR, () => Promise.reject(new Error("no service")), f.deps),
  );
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).toHaveBeenCalledOnce();
  await waitFor(() => expect(result.current.prompt).toEqual({ mode: "unread" }));
  expect(f.invoke).toHaveBeenCalledWith("proxy_status", { host: "127.0.0.1", port: 7420, own: NO_QUIT_FACTS.ownAddresses });
});

test("Cancel dismisses the prompt and does not quit", async () => {
  const f = fakeDeps("ours");
  const { result } = confirmFor(f);
  await closeAndWait(f);
  act(() => result.current.cancel());
  await waitFor(() => expect(result.current.prompt).toBeNull());
  expect(f.destroy).not.toHaveBeenCalled();
});

// The user is never trapped, but a failed unset must not quit: quitting on a
// proxy that is still up is the stranding this dialog exists to prevent. The
// rejection reaches the dialog, which offers the manual route and keeps `quit`
// available beside "try again" — see ProxyCloseDialog.test.tsx.
test("a revert that fails does not quit — it surfaces, and quit stays a click away", async () => {
  const f = fakeDeps("ours");
  f.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "proxy_status") return { status: "ours", address: null };
    throw new Error("elevation declined");
  });
  const { result } = confirmFor(f);
  await closeAndWait(f);
  await act(async () => {
    await expect(result.current.stop()).rejects.toThrow("elevation declined");
  });
  expect(f.destroy).not.toHaveBeenCalled();

  await act(async () => {
    await result.current.quit(false);
  });
  expect(f.destroy).toHaveBeenCalledOnce();
});

// Review round 1: a read that never settles may not hold the close.
describe("every read a close waits on is bounded", () => {
  test("a read that answers in time is its answer; one that fails or never settles is the fallback", async () => {
    vi.useFakeTimers();
    try {
      await expect(bounded(Promise.resolve("read"), "fallback")).resolves.toBe("read");
      await expect(bounded(Promise.reject(new Error("no")), "fallback")).resolves.toBe("fallback");
      const pending = bounded(new Promise<string>(() => {}), "fallback");
      await vi.advanceTimersByTimeAsync(QUIT_READ_MS);
      await expect(pending).resolves.toBe("fallback");
    } finally {
      vi.useRealTimers();
    }
  });

  test("facts that never arrive still let the close ask on this machine's reading", async () => {
    vi.useFakeTimers();
    try {
      const f = fakeDeps("ours");
      const { result } = renderHook(() => useProxyCloseConfirm(ADDR, () => new Promise<QuitFacts>(() => {}), f.deps));
      const preventDefault = vi.fn();
      await act(async () => {
        const closing = f.fireClose()({ preventDefault });
        await vi.advanceTimersByTimeAsync(2 * QUIT_READ_MS);
        await closing;
      });
      expect(preventDefault).toHaveBeenCalledOnce();
      expect(result.current.prompt).toEqual({ mode: "ours", answering: true });
    } finally {
      vi.useRealTimers();
    }
  });
});
