import { act, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useProxyCloseConfirm } from "./closeConfirm";
import { CLOSE_COPY, ProxyCloseDialog } from "./ProxyCloseDialog";

const ADDR = "127.0.0.1:7420";

// A fake Tauri window + invoke: captures the onCloseRequested handler so a test can
// fire a close, stubs proxy_status / remote_notice_dismissed per case, and records
// which commands + destroy() ran.
function fakeDeps(status: "ours" | "foreign" | "none", noticeDismissed = false) {
  let closeHandler: (e: { preventDefault: () => void }) => void = () => {};
  const destroy = vi.fn(async () => {});
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "proxy_status") return status;
    if (cmd === "remote_notice_dismissed") return noticeDismissed;
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

test("ours: prevents the quit and opens the Remove/Keep prompt", async () => {
  const f = fakeDeps("ours");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).toHaveBeenCalledOnce();
  await waitFor(() => expect(result.current.mode).toBe("remove"));
  expect(f.invoke).toHaveBeenCalledWith("proxy_status", { host: "127.0.0.1", port: 7420 });
});

test("ours + Remove: reverts the proxy then quits", async () => {
  const f = fakeDeps("ours");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.remove();
  });
  expect(f.invoke).toHaveBeenCalledWith("unset_proxy");
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("ours + Keep: leaves the proxy up and just quits — no unset, no persisted state", async () => {
  const f = fakeDeps("ours");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.keep();
  });
  expect(f.invoke).not.toHaveBeenCalledWith("unset_proxy");
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("foreign: shows the info-only notice (no Remove), since a device may still point here", async () => {
  const f = fakeDeps("foreign");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).toHaveBeenCalledOnce();
  await waitFor(() => expect(result.current.mode).toBe("notice"));
});

test("none: shows the info-only notice too (remote device might route here)", async () => {
  const f = fakeDeps("none");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await waitFor(() => expect(result.current.mode).toBe("notice"));
});

test("none + notice already dismissed: quits without asking", async () => {
  const f = fakeDeps("none", true);
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  const preventDefault = await closeAndWait(f);
  expect(preventDefault).not.toHaveBeenCalled();
  expect(result.current.mode).toBeNull();
});

test("notice + don't-show-again: persists the dismissal then quits", async () => {
  const f = fakeDeps("none");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.acknowledge(true);
  });
  expect(f.invoke).toHaveBeenCalledWith("dismiss_remote_notice");
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("notice + quit without checking the box: quits, does not persist", async () => {
  const f = fakeDeps("none");
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.acknowledge(false);
  });
  expect(f.invoke).not.toHaveBeenCalledWith("dismiss_remote_notice");
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

test("a revert that fails still quits (never traps the user)", async () => {
  const f = fakeDeps("ours");
  f.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "proxy_status") return "ours";
    throw new Error("elevation declined");
  });
  const { result } = renderHook(() => useProxyCloseConfirm(ADDR, f.deps));
  await closeAndWait(f);
  await act(async () => {
    await result.current.remove();
  });
  expect(f.destroy).toHaveBeenCalledOnce();
});

test("remove-mode dialog: Keep/Remove buttons, internet-loss copy, Esc + backdrop cancel", () => {
  const onRemove = vi.fn();
  const onKeep = vi.fn();
  const onAcknowledge = vi.fn();
  const onCancel = vi.fn();
  render(
    <ProxyCloseDialog
      mode="remove"
      os="macos"
      onRemove={onRemove}
      onKeep={onKeep}
      onAcknowledge={onAcknowledge}
      onCancel={onCancel}
    />,
  );
  expect(screen.getByTestId("proxy-close-dialog")).toHaveTextContent(/internet may stop working/);
  expect(screen.getByTestId("proxy-close-remove")).toHaveTextContent(CLOSE_COPY.remove.confirm);
  expect(screen.getByTestId("proxy-close-keep")).toHaveTextContent(CLOSE_COPY.remove.keep);
  expect(screen.queryByTestId("proxy-close-steps")).toBeNull();

  fireEvent.click(screen.getByTestId("proxy-close-remove"));
  fireEvent.click(screen.getByTestId("proxy-close-keep"));
  fireEvent.click(screen.getByTestId("proxy-close-backdrop"));
  fireEvent.keyDown(screen.getByTestId("proxy-close-dialog"), { key: "Escape" });
  expect(onRemove).toHaveBeenCalledOnce();
  expect(onKeep).toHaveBeenCalledOnce();
  expect(onCancel).toHaveBeenCalledTimes(2);
  expect(onAcknowledge).not.toHaveBeenCalled();
});

test("notice-mode dialog: removal steps, don't-show-again checkbox, single Quit that acknowledges", () => {
  const onAcknowledge = vi.fn();
  render(
    <ProxyCloseDialog
      mode="notice"
      os="macos"
      onRemove={vi.fn()}
      onKeep={vi.fn()}
      onAcknowledge={onAcknowledge}
      onCancel={vi.fn()}
    />,
  );
  expect(screen.getByTestId("proxy-close-steps")).toBeInTheDocument();
  expect(screen.queryByTestId("proxy-close-remove")).toBeNull();

  fireEvent.click(screen.getByTestId("proxy-close-dont-show"));
  fireEvent.click(screen.getByTestId("proxy-close-quit"));
  expect(onAcknowledge).toHaveBeenCalledWith(true);
});
