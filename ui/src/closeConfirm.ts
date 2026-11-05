import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { KeepCapturing } from "./generalPrefs";
import { splitProxyAddr } from "./setup";
import { dismissNotice, noticeDismissed, NOTICE_KEY, type NoticeKey } from "./noticePreference";

// Quit-time proxy check (E-UISETUP.W5, re-ruled by E-ONBRVP.W11). Keys off the
// ACTUAL system proxy at the moment of the close, never off what this session
// did, and asks only where a quit leaves something without internet:
//   "ours"    → this machine routes through Trawler — at its address, localhost,
//               or one of this machine's own addresses at Trawler's port (D3).
//   "unread"  → nothing here could read this machine's proxy (D2): the dialog
//               says so rather than "no proxy is set".
//   "devices" → nothing here routes through Trawler, but a device could: reach
//               is armed, or a running emulator's own proxy points here (D4).
// A proxy that is not Trawler's asks nothing: a quit changes nothing for it.

type CloseEvent = { preventDefault: () => void };
type Win = { onCloseRequested: (h: (e: CloseEvent) => void) => Promise<() => void>; destroy: () => Promise<void> };
type Deps = {
  getWindow: () => Win;
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  listen?: (event: string, handler: () => void) => Promise<() => void>;
};

const real: Deps = { getWindow: getCurrentWindow as unknown as () => Win, invoke, listen };

export const QUIT_REQUEST_EVENT = "quit-requested";

export type RequestKind = "close" | "quit";
export type CloseDecision = "closes" | "asks-keep" | "checks";

// ✕ asks to close the window; ⌘Q and the menu-bar's Quit ask to quit the app.
// Only a close can leave the app running, so only a close can skip the check
// or ask the once-question (DEC-64 rules 1 and 6, DEC-77). A reading that
// failed is neither "on" nor "unanswered": it checks, the safe side.
export function closeDecision(kind: RequestKind, keep: KeepCapturing, tray: boolean): CloseDecision {
  if (kind === "quit" || !tray) return "checks";
  if (keep === "on") return "closes";
  return keep === "unanswered" ? "asks-keep" : "checks";
}

export type QuitPrompt =
  | { mode: "ours"; answering: boolean }
  | { mode: "unread" }
  | { mode: "devices"; emulator: string | null };

// What the shell knows at the moment of the close, beyond this machine's proxy.
// `answering` and the reach facts come from one read of the capture service, so
// a service that does not answer has no armed address and no addresses at all.
export interface QuitFacts {
  answering: boolean;
  ownAddresses: string[];
  reachArmed: boolean;
  emulator: string | null;
}

// A read that failed claims nothing: not that the service is down, not that
// any device could route here.
export const NO_QUIT_FACTS: QuitFacts = { answering: true, ownAddresses: [], reachArmed: false, emulator: null };

// Every read a close waits on is bounded — the facts, the proxy and the opt-out
// alike: a close held by a read that never settles is a window that cannot be
// quit (W11's review, rounds 1 and 2).
export const QUIT_READ_MS = 1500;

export function bounded<T>(read: Promise<T>, fallback: T, ms = QUIT_READ_MS): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<T>((resolve) => {
    timer = setTimeout(() => resolve(fallback), ms);
  });
  return Promise.race([read.catch(() => fallback), late]).finally(() => clearTimeout(timer));
}

const dismissKey = (prompt: QuitPrompt): NoticeKey | null =>
  prompt.mode === "unread" ? NOTICE_KEY.unread : prompt.mode === "devices" ? NOTICE_KEY.remote : null;

// Each stake in the order a quit asks about it. An opt-out silences its own
// stake only: a device that routes here is asked about even where this
// machine's unread proxy no longer is (DEC-64 rules 4 and 5).
function promptsFor(status: string, facts: QuitFacts): QuitPrompt[] {
  if (status === "ours") return [{ mode: "ours", answering: facts.answering }];
  const devices: QuitPrompt[] =
    facts.reachArmed || facts.emulator !== null ? [{ mode: "devices", emulator: facts.emulator }] : [];
  return status === "unknown" ? [{ mode: "unread" }, ...devices] : devices;
}

export function promptFor(status: string, facts: QuitFacts): QuitPrompt | null {
  return promptsFor(status, facts)[0] ?? null;
}

async function firstAsked(
  prompts: QuitPrompt[],
  dismissed: (key: NoticeKey) => Promise<boolean>,
): Promise<QuitPrompt | null> {
  for (const prompt of prompts) {
    const key = dismissKey(prompt);
    if (key === null || !(await dismissed(key))) return prompt;
  }
  return null;
}

export function useProxyCloseConfirm(
  proxyAddr: string,
  readFacts: () => Promise<QuitFacts>,
  deps: Deps = real,
) {
  const [prompt, setPrompt] = useState<QuitPrompt | null>(null);
  const [asking, setAsking] = useState(false);
  const ending = useRef<RequestKind>("close");
  const question = useRef<() => Promise<QuitPrompt | null>>(async () => null);

  // A window that refuses to close is not something the user can act on from a
  // dialog that is about the proxy, so every quit path logs and moves on rather
  // than leaving an unhandled rejection behind.
  const destroy = (): Promise<void> =>
    deps
      .getWindow()
      .destroy()
      .catch((err: unknown) => {
        console.error("window destroy failed:", err);
      });
  const quitApp = async (): Promise<void> => {
    try {
      await deps.invoke("quit_app");
    } catch (err) {
      console.error("quitting failed:", err);
    }
  };
  const end = (): Promise<void> => (ending.current === "quit" ? quitApp() : destroy());

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenQuit: (() => void) | undefined;
    let disposed = false;
    const { host, port } = splitProxyAddr(proxyAddr);

    const firstQuestion = async (): Promise<QuitPrompt | null> => {
      const facts = await bounded(readFacts(), NO_QUIT_FACTS, 2 * QUIT_READ_MS);
      // A shell that cannot answer has read nothing, and a handler that throws
      // leaves a window that will not close: unread is the honest reading.
      const status = await bounded(
        deps
          .invoke<{ status: string }>("proxy_status", { host, port, own: facts.ownAddresses })
          .then((reading) => reading?.status ?? "unknown"),
        "unknown",
      );
      return firstAsked(promptsFor(status, facts), (key) => bounded(noticeDismissed(key, deps.invoke), false));
    };

    question.current = firstQuestion;

    const answerQuitRequest = async () => {
      setAsking(false);
      ending.current = "quit";
      const asked = await firstQuestion();
      if (asked === null) await quitApp();
      else setPrompt(asked);
    };

    const takeQuitRequest = async () => {
      if (await bounded(deps.invoke<boolean>("take_quit_request"), false)) await answerQuitRequest();
    };

    deps
      .getWindow()
      .onCloseRequested(async (e) => {
        const [keep, tray] = await Promise.all([
          bounded(deps.invoke<KeepCapturing>("keep_capturing_read"), "off"),
          bounded(deps.invoke<boolean>("tray_available"), false),
        ]);
        const decision = closeDecision("close", keep, tray === true);
        if (decision === "closes") return;
        ending.current = "close";
        if (decision === "asks-keep") {
          e.preventDefault();
          setAsking(true);
          return;
        }
        const asked = await firstQuestion();
        if (asked === null) return;
        e.preventDefault();
        setPrompt(asked);
      })
      .then((u) => (disposed ? u() : (unlisten = u)));

    void takeQuitRequest();
    deps
      .listen?.(QUIT_REQUEST_EVENT, () => void takeQuitRequest())
      .then((u) => (disposed ? u() : (unlistenQuit = u)))
      .catch((err: unknown) => {
        console.error("listening for quit requests failed:", err);
      });

    return () => {
      disposed = true;
      unlisten?.();
      unlistenQuit?.();
    };
  }, [deps, proxyAddr, readFacts]);

  return {
    prompt,
    asking,
    // A failed unset must not quit: the dialog exists to stop the machine being
    // stranded on a dead proxy, and swallowing the error strands it silently.
    // The rejection reaches the dialog, which offers the manual route instead.
    // Only the unset itself reports failure — once the proxy IS off, a window
    // that will not close is not "couldn't stop routing", and telling the user
    // to undo it by hand would be advice to undo nothing.
    stop: () => deps.invoke("unset_proxy").then(end),
    // The opt-out persists only where the dialog offered one.
    quit: (dontShowAgain: boolean) => {
      const key = prompt && dismissKey(prompt);
      const remembered =
        dontShowAgain && key !== null ? dismissNotice(key, deps.invoke).catch(() => {}) : Promise.resolve();
      return remembered.then(end);
    },
    cancel: () => setPrompt(null),
    // The answer is the switch. A write that fails leaves the window open, as a
    // cancel does: closing into a state nothing recorded would ask again blind.
    keepCapturing: async () => {
      setAsking(false);
      try {
        await deps.invoke("keep_capturing_write", { on: true });
      } catch (err) {
        console.error("storing keep capturing failed:", err);
        return;
      }
      await destroy();
    },
    // Quit is the user's answer even where it could not be stored: the next
    // close simply asks again.
    quitInstead: async () => {
      setAsking(false);
      ending.current = "close";
      await deps.invoke("keep_capturing_write", { on: false }).catch((err: unknown) => {
        console.error("storing keep capturing failed:", err);
      });
      const asked = await question.current();
      if (asked === null) await destroy();
      else setPrompt(asked);
    },
    cancelAsk: () => setAsking(false),
  };
}
