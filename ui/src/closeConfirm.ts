import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { splitProxyAddr } from "./setup";

// Quit-time proxy check (E-UISETUP.W5). Keys off the ACTUAL system proxy status,
// not off whether we set it this session — so a proxy the user pointed at Trawler
// by hand is caught too, and a stale in-session flag never misleads. On close we
// ask Rust `proxy_status(host, port)`, which classifies the live system proxy:
//   "ours"    → a proxy on THIS system routes through Trawler. Quitting silently
//               would strand the machine on a dead proxy (no internet), so prevent
//               the close and confirm: Remove (unset_proxy, elevation) or Keep.
//   "foreign" → some other proxy is active; not ours to remove, but a device
//               elsewhere might still point here.
//   "none"    → this system is clean, but the proxy could live on another device
//               (phone / emulator pointed at this machine's LAN IP) we can't read.
// The last two can't be auto-fixed from here, so they show an info-only notice
// (how to check / remove) with a durable "don't show again" — never a Remove
// button, since there's nothing local to remove.

type CloseEvent = { preventDefault: () => void };
type Win = { onCloseRequested: (h: (e: CloseEvent) => void) => Promise<() => void>; destroy: () => Promise<void> };
type Deps = { getWindow: () => Win; invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };

const real: Deps = { getWindow: getCurrentWindow as unknown as () => Win, invoke };

// null = no prompt (quit proceeds); "remove" = ours, offer Remove/Keep; "notice" =
// info-only (can't auto-fix, warn once).
export type CloseMode = "remove" | "notice";

export function useProxyCloseConfirm(proxyAddr: string, deps: Deps = real) {
  const [mode, setMode] = useState<CloseMode | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    const { host, port } = splitProxyAddr(proxyAddr);
    deps
      .getWindow()
      .onCloseRequested(async (e) => {
        const status = await deps.invoke<string>("proxy_status", { host, port });
        if (status === "ours") {
          e.preventDefault();
          setMode("remove");
        } else if (!(await deps.invoke<boolean>("remote_notice_dismissed"))) {
          e.preventDefault();
          setMode("notice");
        }
      })
      .then((u) => (disposed ? u() : (unlisten = u)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [deps, proxyAddr]);

  const quit = () => deps.getWindow().destroy();

  return {
    mode,
    remove: () => deps.invoke("unset_proxy").catch(() => {}).then(quit),
    // Live proxy state is the only source of truth (no persisted marker): keeping
    // the proxy is just quitting — there is nothing to record, and next launch
    // does no reconciliation.
    keep: quit,
    // Info-only notice: optionally persist "don't show again", then quit.
    acknowledge: (dontShowAgain: boolean) =>
      (dontShowAgain ? deps.invoke("dismiss_remote_notice").catch(() => {}) : Promise.resolve()).then(quit),
    cancel: () => setMode(null),
  };
}
