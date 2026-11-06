import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import {
  CLOSE_WINDOW_EVENT,
  DIALOG_REQUEST_EVENT,
  enqueue,
  topmost,
  useCloseWindowRequest,
  useShellDialogs,
  type ShellDialog,
} from "./shellDialogs";

const SETTINGS: ShellDialog = { dialog: "settings" };
const about = (page: "about" | "licences" | "privacy"): ShellDialog => ({ dialog: "about", page, unopened: null });

function shell(pending: ShellDialog[][]) {
  const handlers = new Map<string, () => void>();
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "take_dialog_requests") return pending.shift() ?? [];
    throw new Error(`${cmd} is not stubbed`);
  });
  const listen = vi.fn(async (event: string, handler: () => void) => {
    handlers.set(event, handler);
    return () => handlers.delete(event);
  });
  const deps = { invoke: invoke as never, listen };
  return { deps, announce: (event: string) => act(() => handlers.get(event)?.()), handlers };
}

test("a second ask for a dialog already waiting updates it in place, never a second one", () => {
  expect(enqueue([], SETTINGS)).toEqual([SETTINGS]);
  expect(enqueue([SETTINGS], SETTINGS)).toEqual([SETTINGS]);
  expect(enqueue([SETTINGS], about("about"))).toEqual([SETTINGS, about("about")]);
  expect(enqueue([about("about"), SETTINGS], about("privacy"))).toEqual([about("privacy"), SETTINGS]);
});

test("the quit prompt, then the keep-capturing ask, then a shell dialog, then the window is what closes first", () => {
  expect(topmost({ prompt: true, asking: false, shell: true })).toBe("quit-prompt");
  expect(topmost({ prompt: false, asking: true, shell: true })).toBe("keep-ask");
  expect(topmost({ prompt: false, asking: false, shell: true })).toBe("shell-dialog");
  expect(topmost({ prompt: false, asking: false, shell: false })).toBe("window");
});

test("a request made before the window listened is taken once it does", async () => {
  const { deps } = shell([[SETTINGS]]);
  const { result } = renderHook(() => useShellDialogs(deps));
  await waitFor(() => expect(result.current.current).toEqual(SETTINGS));
});

test("an announced request shows; later ones wait in order behind the one shown", async () => {
  const { deps, announce } = shell([[], [about("licences")], [SETTINGS]]);
  const { result } = renderHook(() => useShellDialogs(deps));
  await waitFor(() => expect(deps.listen).toHaveBeenCalledWith(DIALOG_REQUEST_EVENT, expect.any(Function)));
  await waitFor(() => expect(deps.invoke).toHaveBeenCalledTimes(1));
  expect(result.current.current).toBeNull();

  announce(DIALOG_REQUEST_EVENT);
  await waitFor(() => expect(result.current.current).toEqual(about("licences")));
  announce(DIALOG_REQUEST_EVENT);
  await waitFor(() => expect(deps.invoke).toHaveBeenCalledTimes(3));
  expect(result.current.current).toEqual(about("licences"));

  act(() => result.current.dismiss());
  expect(result.current.current).toEqual(SETTINGS);
  act(() => result.current.dismiss());
  expect(result.current.current).toBeNull();
});

test("Close Window calls the newest handler, and stops listening on unmount", async () => {
  const { deps, announce, handlers } = shell([]);
  const first = vi.fn();
  const second = vi.fn();
  const { rerender, unmount } = renderHook(({ onClose }) => useCloseWindowRequest(onClose, deps), {
    initialProps: { onClose: first },
  });
  await waitFor(() => expect(handlers.has(CLOSE_WINDOW_EVENT)).toBe(true));
  rerender({ onClose: second });
  announce(CLOSE_WINDOW_EVENT);
  expect(first).not.toHaveBeenCalled();
  expect(second).toHaveBeenCalledTimes(1);
  unmount();
  expect(handlers.has(CLOSE_WINDOW_EVENT)).toBe(false);
});
