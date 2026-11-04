import { browser } from "@wdio/globals";

// The app is launched once per run by the launcher, and `reloadSession` keeps that
// same process, so a relaunch is a real quit: destroying the window is the close
// path that saves windows.json, and the app exits with its last window. The
// launcher respawns an app whose WebDriver no longer answers before the next spec
// file's session. The runner skips its own session delete when no session id is
// left, which is the only way it does not fail the file against a server that is
// gone by design; the global `browser` is a read-only proxy, so the id is cleared
// on the instance the runner holds.
export async function quitTheApp() {
  await browser.execute(() => {
    const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { destroy: () => Promise<void> } } } })
      .__TAURI__;
    setTimeout(() => void tauri.window.getCurrentWindow().destroy(), 100);
  });
  const status = `http://${browser.options.hostname ?? "127.0.0.1"}:${browser.options.port}/status`;
  await browser.waitUntil(() => fetch(status).then(() => false, () => true), {
    timeout: 15_000,
    timeoutMsg: "the app never exited after its window was destroyed",
  });
  const runnersBrowser = (globalThis as unknown as { _wdioGlobals: Map<string, { sessionId?: string }> })._wdioGlobals;
  runnersBrowser.get("browser")!.sessionId = undefined;
}
