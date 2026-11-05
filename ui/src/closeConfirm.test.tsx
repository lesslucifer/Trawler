import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import {
  bounded,
  closeDecision,
  NO_QUIT_FACTS,
  promptFor,
  QUIT_READ_MS,
  useProxyCloseConfirm,
  type QuitFacts,
} from "./closeConfirm";
import type { KeepCapturing } from "./generalPrefs";

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

test("unknown with its opt-out taken still asks about a device that routes here", async () => {
  const f = fakeDeps("unknown", ["unread-notice"]);
  const { result } = confirmFor(f, { reachArmed: true });
  await closeAndWait(f);
  await waitFor(() => expect(result.current.prompt).toEqual({ mode: "devices", emulator: null }));

  const both = fakeDeps("unknown", ["unread-notice", "remote-notice"]);
  const silent = confirmFor(both, { emulator: "Pixel_7_API_35" });
  expect(await closeAndWait(both)).not.toHaveBeenCalled();
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

  // Review round 2: the shell's own reads are waited on too.
  test.each([
    ["proxy_status", { mode: "unread" }],
    ["notice_dismissed", { mode: "unread" }],
  ] as const)("a %s that never answers still lets the close ask", async (hung, asked) => {
    vi.useFakeTimers();
    try {
      const f = fakeDeps("unknown");
      f.invoke.mockImplementation(async (cmd: string) => {
        if (cmd === hung) return new Promise(() => {});
        if (cmd === "proxy_status") return { status: "unknown", address: null };
        return false;
      });
      const { result } = confirmFor(f);
      const preventDefault = vi.fn();
      await act(async () => {
        const closing = f.fireClose()({ preventDefault });
        await vi.advanceTimersByTimeAsync(2 * QUIT_READ_MS);
        await closing;
      });
      expect(preventDefault).toHaveBeenCalledOnce();
      expect(result.current.prompt).toEqual(asked);
    } finally {
      vi.useRealTimers();
    }
  });
});

function shellDeps(
  status: "ours" | "foreign" | "none" | "unknown",
  shell: { keep?: KeepCapturing; tray?: boolean; quitPending?: boolean; writeFails?: boolean; unreadable?: boolean; holdListen?: boolean; holdReads?: boolean; holdStatus?: boolean; holdWrite?: boolean; holdSecondStatus?: boolean } = {},
) {
  const f = fakeDeps(status);
  let keep: KeepCapturing = shell.keep ?? "off";
  let pending = shell.quitPending ?? false;
  let announce = () => {};
  let listening: () => void = () => {};
  let releasing: () => void = () => {};
  let releaseStatus: () => void = () => {};
  const statusReleased = new Promise<void>((resolve) => (releaseStatus = resolve));
  let statusHeld = false;
  let statusCalls = 0;
  let releaseSecond: () => void = () => {};
  const secondReleased = new Promise<void>((resolve) => (releaseSecond = resolve));
  let releaseWrite: () => void = () => {};
  const writeReleased = new Promise<void>((resolve) => (releaseWrite = resolve));
  const readsReleased = new Promise<void>((resolve) => (releasing = resolve));
  const listenStands = new Promise<void>((resolve) => (listening = resolve));
  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "keep_capturing_read" || cmd === "tray_available") {
      if (shell.unreadable) throw new Error("no shell");
      if (shell.holdReads) await readsReleased;
      return cmd === "tray_available" ? (shell.tray ?? true) : keep;
    }
    if (cmd === "proxy_status" && shell.holdSecondStatus && ++statusCalls === 2) await secondReleased;
    if (cmd === "proxy_status" && shell.holdStatus && !statusHeld) {
      statusHeld = true;
      await statusReleased;
      return { status: "none", address: null };
    }
    if (cmd === "take_quit_request") {
      const was = pending;
      pending = false;
      return was;
    }
    if (cmd === "keep_capturing_write") {
      if (shell.holdWrite) await writeReleased;
      if (shell.writeFails) throw new Error("permission denied");
      keep = args?.on ? "on" : "off";
      return undefined;
    }
    return f.invoke(cmd, args);
  });
  const deps = {
    ...f.deps,
    invoke: invoke as never,
    listen: async (_event: string, handler: () => void) => {
      if (shell.holdListen) await listenStands;
      announce = handler;
      return () => {};
    },
  };
  const proxyReads = () => invoke.mock.calls.filter(([cmd]) => cmd === "proxy_status").length;
  const ran = (cmd: string) => invoke.mock.calls.some(([name]) => name === cmd);
  return {
    ...f,
    deps,
    invoke,
    proxyReads,
    ran,
    releaseReads: () => releasing(),
    releaseStatus: () => releaseStatus(),
    releaseSecondStatus: () => releaseSecond(),
    releaseWrite: () => releaseWrite(),
    registerListener: () => act(async () => listening()),
    announce: () => act(async () => announce()),
    request: async () => {
      pending = true;
      await act(async () => announce());
    },
  };
}

function shellConfirm(f: ReturnType<typeof shellDeps>) {
  return renderHook(() => useProxyCloseConfirm(ADDR, async () => FACTS, f.deps));
}

describe("closeDecision", () => {
  test.each([
    ["close", "on", true, "closes"],
    ["close", "unanswered", true, "asks-keep"],
    ["close", "off", true, "checks"],
    ["close", "on", false, "checks"],
    ["close", "unanswered", false, "checks"],
    ["close", "off", false, "checks"],
    ["quit", "on", true, "checks"],
    ["quit", "unanswered", true, "checks"],
    ["quit", "off", true, "checks"],
    ["quit", "on", false, "checks"],
  ] as const)("%s · keep %s · tray %s → %s", (kind, keep, tray, decision) => {
    expect(closeDecision(kind, keep, tray)).toBe(decision);
  });
});

describe("✕ with the switch on and a menu-bar icon", () => {
  test("closes at once and reads no proxy", async () => {
    const f = shellDeps("ours", { keep: "on" });
    const { result } = shellConfirm(f);
    const preventDefault = await closeAndWait(f);
    expect(preventDefault).not.toHaveBeenCalled();
    expect(f.proxyReads()).toBe(0);
    expect(result.current.prompt).toBeNull();
    expect(result.current.asking).toBe(false);
  });
});

describe("the once-question", () => {
  test("an unanswered switch with an icon asks before anything is read", async () => {
    const f = shellDeps("ours", { keep: "unanswered" });
    const { result } = shellConfirm(f);
    const preventDefault = await closeAndWait(f);
    expect(preventDefault).toHaveBeenCalledOnce();
    expect(result.current.asking).toBe(true);
    expect(f.proxyReads()).toBe(0);
  });

  test("with no icon nothing asks: the check runs as DEC-64 has it", async () => {
    const f = shellDeps("none", { keep: "unanswered", tray: false });
    const { result } = shellConfirm(f);
    const preventDefault = await closeAndWait(f);
    expect(preventDefault).not.toHaveBeenCalled();
    expect(result.current.asking).toBe(false);
  });

  test("keep capturing stores on, closes, and reads no proxy", async () => {
    const f = shellDeps("ours", { keep: "unanswered" });
    const { result } = shellConfirm(f);
    await closeAndWait(f);
    await act(async () => result.current.keepCapturing());
    expect(f.invoke).toHaveBeenCalledWith("keep_capturing_write", { on: true });
    expect(f.destroy).toHaveBeenCalledOnce();
    expect(result.current.asking).toBe(false);
    expect(f.proxyReads()).toBe(0);
  });

  test("quit stores off, then closes where nothing is routed here", async () => {
    const f = shellDeps("none", { keep: "unanswered" });
    const { result } = shellConfirm(f);
    await closeAndWait(f);
    await act(async () => result.current.quitInstead());
    expect(f.invoke).toHaveBeenCalledWith("keep_capturing_write", { on: false });
    expect(f.destroy).toHaveBeenCalledOnce();
    expect(result.current.prompt).toBeNull();
  });

  test("quit then runs DEC-64's check: the dialog while routed here, the close on its quit", async () => {
    const f = shellDeps("ours", { keep: "unanswered" });
    const { result } = shellConfirm(f);
    await closeAndWait(f);
    await act(async () => result.current.quitInstead());
    expect(result.current.prompt).toEqual({ mode: "ours", answering: true });
    expect(f.destroy).not.toHaveBeenCalled();
    await act(async () => result.current.quit(false));
    expect(f.destroy).toHaveBeenCalledOnce();
    expect(f.ran("quit_app")).toBe(false);
  });

  test("cancelling stores nothing, keeps the window, and the next ✕ asks again", async () => {
    const f = shellDeps("ours", { keep: "unanswered" });
    const { result } = shellConfirm(f);
    await closeAndWait(f);
    act(() => result.current.cancelAsk());
    expect(result.current.asking).toBe(false);
    expect(f.ran("keep_capturing_write")).toBe(false);
    expect(f.destroy).not.toHaveBeenCalled();
    await closeAndWait(f);
    expect(result.current.asking).toBe(true);
  });

  test("a refusal does not outlive its question: a quit request closes it, and the next ✕ asks clean", async () => {
    const f = shellDeps("none", { keep: "unanswered", writeFails: true });
    const { result } = shellConfirm(f);
    await closeAndWait(f);
    await act(async () => result.current.keepCapturing());
    expect(result.current.keepRefused).toBe("permission denied");
    await f.request();
    expect(result.current.asking).toBe(false);
    await closeAndWait(f);
    expect(result.current.asking).toBe(true);
    expect(result.current.keepRefused).toBeNull();
  });

  test("an answer that cannot be stored leaves the window open when keeping, and still quits when quitting", async () => {
    const keeping = shellDeps("none", { keep: "unanswered", writeFails: true });
    const kept = shellConfirm(keeping);
    await closeAndWait(keeping);
    await act(async () => kept.result.current.keepCapturing());
    expect(keeping.destroy).not.toHaveBeenCalled();
    expect(kept.result.current.asking).toBe(true);
    expect(kept.result.current.keepRefused).toBe("permission denied");
    act(() => kept.result.current.cancelAsk());
    expect(kept.result.current.keepRefused).toBeNull();

    const quitting = shellDeps("none", { keep: "unanswered", writeFails: true });
    const quit = shellConfirm(quitting);
    await closeAndWait(quitting);
    await act(async () => quit.result.current.quitInstead());
    expect(quitting.destroy).toHaveBeenCalledOnce();
  });
});

describe("a quit request", () => {
  test.each(["on", "unanswered", "off"] as const)(
    "with the switch %s runs the check, never asks the once-question, and quits the app",
    async (keep) => {
      const f = shellDeps("none", { keep });
      const { result } = shellConfirm(f);
      await f.request();
      await waitFor(() => expect(f.ran("quit_app")).toBe(true));
      expect(f.proxyReads()).toBe(1);
      expect(result.current.asking).toBe(false);
      expect(f.destroy).not.toHaveBeenCalled();
    },
  );

  test("routed through Trawler it asks first; quit and stop each end in quit_app, cancel does not", async () => {
    const quitting = shellDeps("ours", { keep: "on" });
    const quit = shellConfirm(quitting);
    await quitting.request();
    await waitFor(() => expect(quit.result.current.prompt).toEqual({ mode: "ours", answering: true }));
    expect(quitting.ran("quit_app")).toBe(false);
    await act(async () => quit.result.current.quit(false));
    expect(quitting.ran("quit_app")).toBe(true);
    expect(quitting.destroy).not.toHaveBeenCalled();

    const stopping = shellDeps("ours", { keep: "on" });
    const stop = shellConfirm(stopping);
    await stopping.request();
    await waitFor(() => expect(stop.result.current.prompt).not.toBeNull());
    await act(async () => stop.result.current.stop());
    const order = stopping.invoke.mock.calls.map(([cmd]) => cmd);
    expect(order.indexOf("unset_proxy")).toBeGreaterThan(-1);
    expect(order.indexOf("quit_app")).toBeGreaterThan(order.indexOf("unset_proxy"));

    const cancelling = shellDeps("ours", { keep: "on" });
    const cancel = shellConfirm(cancelling);
    await cancelling.request();
    await waitFor(() => expect(cancel.result.current.prompt).not.toBeNull());
    act(() => cancel.result.current.cancel());
    expect(cancelling.ran("quit_app")).toBe(false);
    expect(cancel.result.current.prompt).toBeNull();
  });

  test("one pending at mount quits once, and an event that finds the flag taken does nothing", async () => {
    const f = shellDeps("none", { keep: "on", quitPending: true });
    shellConfirm(f);
    await waitFor(() => expect(f.ran("quit_app")).toBe(true));
    await f.announce();
    expect(f.invoke.mock.calls.filter(([cmd]) => cmd === "quit_app")).toHaveLength(1);
  });

  test("a request that lands before the listener stands is taken once it does", async () => {
    const f = shellDeps("none", { keep: "on", holdListen: true });
    shellConfirm(f);
    await act(async () => {});
    expect(f.ran("take_quit_request")).toBe(false);
    await f.request();
    await f.registerListener();
    await waitFor(() => expect(f.ran("quit_app")).toBe(true));
  });

  test("a quit request that arrives while a ✕ is still reading keeps the quit's ending", async () => {
    const f = shellDeps("ours", { keep: "on", holdReads: true });
    const { result } = shellConfirm(f);
    const preventDefault = vi.fn();
    const closing = Promise.resolve(f.fireClose()({ preventDefault }));
    await f.request();
    await waitFor(() => expect(result.current.prompt).not.toBeNull());
    f.releaseReads();
    await act(async () => {
      await closing;
    });
    expect(preventDefault).toHaveBeenCalledOnce();
    await act(async () => result.current.quit(false));
    expect(f.ran("quit_app")).toBe(true);
    expect(f.destroy).not.toHaveBeenCalled();
  });

  test("a quit request that arrives while a ✕ is still asking about the proxy is not closed under", async () => {
    const f = shellDeps("ours", { keep: "off", holdStatus: true });
    const { result } = shellConfirm(f);
    const preventDefault = vi.fn();
    const closing = Promise.resolve(f.fireClose()({ preventDefault }));
    await waitFor(() => expect(f.proxyReads()).toBe(1));
    await f.request();
    await waitFor(() => expect(result.current.prompt).not.toBeNull());
    f.releaseStatus();
    await act(async () => {
      await closing;
    });
    expect(preventDefault).toHaveBeenCalledOnce();
    await act(async () => result.current.quit(false));
    expect(f.ran("quit_app")).toBe(true);
    expect(f.destroy).not.toHaveBeenCalled();
  });

  test.each(["keepCapturing", "quitInstead"] as const)(
    "a quit request that lands while the once-question's %s answer is being stored wins",
    async (answer) => {
      const f = shellDeps("none", { keep: "unanswered", holdWrite: true });
      const { result } = shellConfirm(f);
      await closeAndWait(f);
      expect(result.current.asking).toBe(true);
      const answering = Promise.resolve(result.current[answer]());
      await f.request();
      await waitFor(() => expect(f.ran("quit_app")).toBe(true));
      f.releaseWrite();
      await act(async () => {
        await answering;
      });
      expect(f.destroy).not.toHaveBeenCalled();
    },
  );

  test("a keep write that never settles is called too late, and the question's other answers work again", async () => {
    vi.useFakeTimers();
    try {
      const f = shellDeps("none", { keep: "unanswered", holdWrite: true });
      const { result } = shellConfirm(f);
      await closeAndWait(f);
      const keeping = Promise.resolve(result.current.keepCapturing());
      await act(async () => {
        await vi.advanceTimersByTimeAsync(3000);
        await keeping;
      });
      expect(result.current.keepRefused).toBe("it took too long");
      expect(f.destroy).not.toHaveBeenCalled();
      act(() => result.current.cancelAsk());
      expect(result.current.asking).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  test.each(["cancelAsk", "quitInstead"] as const)(
    "%s while keep capturing is being stored waits: the stored switch and the window agree",
    async (other) => {
      const f = shellDeps("none", { keep: "unanswered", holdWrite: true });
      const { result } = shellConfirm(f);
      await closeAndWait(f);
      const keeping = Promise.resolve(result.current.keepCapturing());
      await act(async () => {
        await result.current[other]();
      });
      expect(f.invoke).not.toHaveBeenCalledWith("keep_capturing_write", { on: false });
      expect(result.current.asking).toBe(true);
      f.releaseWrite();
      await act(async () => {
        await keeping;
      });
      expect(f.destroy).toHaveBeenCalledOnce();
    },
  );

  test("a ✕ while a quit prompt is open changes nothing and the quit still ends in quit_app", async () => {
    const f = shellDeps("ours", { keep: "unanswered" });
    const { result } = shellConfirm(f);
    await f.request();
    await waitFor(() => expect(result.current.prompt).not.toBeNull());
    const preventDefault = await closeAndWait(f);
    expect(preventDefault).toHaveBeenCalledOnce();
    expect(result.current.asking).toBe(false);
    await act(async () => result.current.quit(false));
    expect(f.ran("quit_app")).toBe(true);
    expect(f.destroy).not.toHaveBeenCalled();
  });

  test("cancelling one quit prompt does not hand the window back while a second request is still answering", async () => {
    const f = shellDeps("ours", { keep: "on", holdSecondStatus: true });
    const { result } = shellConfirm(f);
    await f.request();
    await waitFor(() => expect(result.current.prompt).not.toBeNull());
    await f.request();
    act(() => result.current.cancel());
    expect(await closeAndWait(f)).toHaveBeenCalledOnce();
    f.releaseSecondStatus();
    await waitFor(() => expect(result.current.prompt).not.toBeNull());
    expect(await closeAndWait(f)).toHaveBeenCalledOnce();
    await act(async () => result.current.quit(false));
    expect(f.ran("quit_app")).toBe(true);
    expect(f.destroy).not.toHaveBeenCalled();
  });

  test("a quit prompt that is cancelled hands the window back to ✕", async () => {
    const f = shellDeps("ours", { keep: "on" });
    const { result } = shellConfirm(f);
    await f.request();
    await waitFor(() => expect(result.current.prompt).not.toBeNull());
    act(() => result.current.cancel());
    const preventDefault = await closeAndWait(f);
    expect(preventDefault).not.toHaveBeenCalled();
  });

  test("a quit request while the once-question is open drops it and checks", async () => {
    const f = shellDeps("none", { keep: "unanswered" });
    const { result } = shellConfirm(f);
    await closeAndWait(f);
    expect(result.current.asking).toBe(true);
    await f.request();
    await waitFor(() => expect(f.ran("quit_app")).toBe(true));
    expect(result.current.asking).toBe(false);
  });
});

test("a shell that cannot read the switch or the icon checks, as a quit always did", async () => {
  const f = shellDeps("ours", { keep: "on", unreadable: true });
  const { result } = shellConfirm(f);
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).toHaveBeenCalledOnce();
  await waitFor(() => expect(result.current.prompt).toEqual({ mode: "ours", answering: true }));
  expect(result.current.asking).toBe(false);
});
