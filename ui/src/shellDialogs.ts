import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AboutShow } from "./About";
import { subscribe, takeOnEvery } from "./shellEvent";

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

// A take that lands after an unmount still counts: the shell has already
// handed those requests over.
export function useShellDialogs(deps: Deps = real) {
  const [queue, setQueue] = useState<ShellDialog[]>([]);

  useEffect(() => {
    const take = () =>
      void deps
        .invoke<ShellDialog[]>("take_dialog_requests")
        .then((asked) => {
          if (Array.isArray(asked)) setQueue((now) => asked.reduce(enqueue, now));
        })
        .catch(() => {});
    return takeOnEvery(deps.listen, DIALOG_REQUEST_EVENT, take);
  }, [deps]);

  const dismiss = useCallback(() => setQueue((now) => now.slice(1)), []);
  return { current: queue[0] ?? null, dismiss };
}

export function useCloseWindowRequest(onClose: () => void, deps: Deps = real) {
  const latest = useRef(onClose);
  latest.current = onClose;
  useEffect(() => subscribe(deps.listen, CLOSE_WINDOW_EVENT, () => latest.current()), [deps]);
}

export const closeThisWindow = (call: Deps["invoke"] = invoke): Promise<void> =>
  call<void>("close_window").catch((err: unknown) => {
    console.error("closing the window failed:", err);
  });
