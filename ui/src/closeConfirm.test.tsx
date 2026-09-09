import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useProxyCloseConfirm } from "./closeConfirm";

const ADDR = "127.0.0.1:7420";

// A fake Tauri window + invoke: captures the onCloseRequested handler so a test can
// fire a close, stubs proxy_status / notice_dismissed per case, and records
// which commands + destroy() ran.
function fakeDeps(status: "ours" | "foreign" | "none", noticeDismissed = false) {
  let closeHandler: (e: { preventDefault: () => void }) => void = () => {};
  const destroy = vi.fn(async () => {});
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "proxy_status") return status;
    if (cmd === "notice_dismissed") return noticeDismissed;
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

async function closeAndWait(f: ReturnType<typeof fakeDeps>) {
  const preventDefault = vi.fn();
  await act(async () => {
    f.fireClose()({ preventDefault });
  });
  return preventDefault;
}

test("ours: prevents the quit and opens the turn-it-off prompt", async () => {
  const f = fakeDeps("ours");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).toHaveBeenCalledOnce();
  await waitFor(() => expect(result.current.mode).toBe("ours"));
  expect(f.invoke).toHaveBeenCalledWith("proxy_status", { host: "127.0.0.1", port: 7420 });
});

test("ours + turn it off: reverts the proxy then quits", async () => {
  const f = fakeDeps("ours");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.remove();
  });
  expect(f.invoke).toHaveBeenCalledWith("unset_proxy");
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("ours + quit: leaves the proxy up and just quits — no unset, no persisted state", async () => {
  const f = fakeDeps("ours");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.keep();
  });
  expect(f.invoke).not.toHaveBeenCalledWith("unset_proxy");
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("foreign: actionable too — unset_proxy turns the OS proxy off whoever set it", async () => {
  const f = fakeDeps("foreign");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).toHaveBeenCalledOnce();
  await waitFor(() => expect(result.current.mode).toBe("foreign"));
});

test("none: the remote notice — nothing local to act on", async () => {
  const f = fakeDeps("none");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await waitFor(() => expect(result.current.mode).toBe("remote"));
});

test("none + remote notice already dismissed: quits without asking", async () => {
  const f = fakeDeps("none", true);
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).not.toHaveBeenCalled();
  expect(result.current.mode).toBeNull();
});

test("remote + don't-show-again: persists the dismissal then quits", async () => {
  const f = fakeDeps("none");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.acknowledge(true);
  });
  expect(f.invoke).toHaveBeenCalledWith("dismiss_notice", { key: "remote-notice" });
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("remote + quit without checking the box: quits, does not persist", async () => {
  const f = fakeDeps("none");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.acknowledge(false);
  });
  expect(f.invoke).not.toHaveBeenCalledWith("dismiss_notice", { key: "remote-notice" });
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("Cancel dismisses the prompt and does not quit", async () => {
  const f = fakeDeps("ours");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  act(() => result.current.cancel());
  await waitFor(() => expect(result.current.mode).toBeNull());
  expect(f.destroy).not.toHaveBeenCalled();
});

// The user is never trapped, but a failed unset must not quit: quitting on a
// proxy that is still up is the stranding this dialog exists to prevent. The
// rejection reaches the dialog, which offers the manual route and keeps `quit`
// available beside "try again" — see ProxyCloseDialog.test.tsx.
test("a revert that fails does not quit — it surfaces, and quit stays a click away", async () => {
  const f = fakeDeps("ours");
  f.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "proxy_status") return "ours";
    throw new Error("elevation declined");
  });
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await expect(result.current.remove()).rejects.toThrow("elevation declined");
  });
  expect(f.destroy).not.toHaveBeenCalled();

  result.current.keep();
  expect(f.destroy).toHaveBeenCalledOnce();
});
