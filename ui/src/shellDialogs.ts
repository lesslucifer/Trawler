import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AboutShow } from "./About";

// Rust tests pin both names to the shell's constants (lifecycle.rs, menu.rs).
export const DIALOG_REQUEST_EVENT = "dialog-requested";
export const CLOSE_WINDOW_EVENT = "close-window-requested";

/** A dialog the shell asks this window to show (W19): Settings, or About on one of its pages. */
export type ShellDialog = { dialog: "settings" } | ({ dialog: "about" } & AboutShow);

type Deps = {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  listen: (event: string, handler: () => void) => Promise<() => void>;
};

const real: Deps = { invoke, listen };

// One of each: asking again for a dialog already waiting or shown updates it in
// place (About's page), so nothing stacks and the newest ask is what shows.
export function enqueue(queue: ShellDialog[], asked: ShellDialog): ShellDialog[] {
  const at = queue.findIndex((waiting) => waiting.dialog === asked.dialog);
  return at === -1 ? [...queue, asked] : queue.map((waiting, i) => (i === at ? asked : waiting));
}

export type Topmost = "quit-prompt" | "keep-ask" | "shell-dialog" | "window";

// The quit prompt and the keep-capturing ask stand over a shell dialog, which
// waits behind them, so those close first.
export function topmost({ prompt, asking, shell }: { prompt: boolean; asking: boolean; shell: boolean }): Topmost {
  if (prompt) return "quit-prompt";
  if (asking) return "keep-ask";
  return shell ? "shell-dialog" : "window";
}

// The quit request's pattern: the flag is taken once the listener stands, and
// on every announcement after. A take that lands after an unmount still counts:
// the shell has already handed those requests over.
export function useShellDialogs(deps: Deps = real) {
  const [queue, setQueue] = useState<ShellDialog[]>([]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const take = () =>
      deps
        .invoke<ShellDialog[]>("take_dialog_requests")
        .then((asked) => {
          if (Array.isArray(asked)) setQueue((now) => asked.reduce(enqueue, now));
        })
        .catch(() => {});
    deps
      .listen(DIALOG_REQUEST_EVENT, () => void take())
      .then((u) => (disposed ? u() : (unlisten = u)))
      .catch((err: unknown) => {
        console.error("listening for dialog requests failed:", err);
      })
      .then(() => {
        if (!disposed) void take();
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [deps]);

  const dismiss = useCallback(() => setQueue((now) => now.slice(1)), []);
  return { current: queue[0] ?? null, dismiss };
}

export function useCloseWindowRequest(onClose: () => void, deps: Deps = real) {
  const latest = useRef(onClose);
  latest.current = onClose;
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    deps
      .listen(CLOSE_WINDOW_EVENT, () => latest.current())
      .then((u) => (disposed ? u() : (unlisten = u)))
      .catch((err: unknown) => {
        console.error("listening for Close Window failed:", err);
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [deps]);
}

export const closeThisWindow = (call: Deps["invoke"] = invoke): Promise<void> =>
  call<void>("close_window").catch((err: unknown) => {
    console.error("closing the window failed:", err);
  });
