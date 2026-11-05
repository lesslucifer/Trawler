import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { refusalWords, type KeepCapturing } from "./generalPrefs";
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
  const [keepRefused, setKeepRefused] = useState<string | null>(null);
  const ending = useRef<RequestKind>("close");
  const quitRequests = useRef(0);
  const quitsAnswering = useRef(0);
  const quitPromptShown = useRef(false);
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
  // quit_app only exits the process and cannot refuse; a rejection means the
  // bridge to the shell is gone, which no dialog here could report through.
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
      quitRequests.current += 1;
      ending.current = "quit";
      quitsAnswering.current += 1;
      try {
        const asked = await firstQuestion();
        if (asked === null) {
          await quitApp();
        } else {
          quitPromptShown.current = true;
          setPrompt(asked);
        }
      } finally {
        quitsAnswering.current -= 1;
      }
    };

    const takeQuitRequest = async () => {
      if (await deps.invoke<boolean>("take_quit_request").catch(() => false)) await answerQuitRequest();
    };

    deps
      .getWindow()
      .onCloseRequested(async (e) => {
        // A quit request owns the window from the moment it arrives: a ✕ that
        // started before it, or lands while its prompt is open, only leaves
        // the window as it is.
        if (quitsAnswering.current > 0 || quitPromptShown.current) {
          e.preventDefault();
          return;
        }
        const requestsSeen = quitRequests.current;
        const [keep, tray] = await Promise.all([
          bounded(deps.invoke<KeepCapturing>("keep_capturing_read"), "off"),
          bounded(deps.invoke<boolean>("tray_available"), false),
        ]);
        if (quitRequests.current !== requestsSeen) {
          e.preventDefault();
          return;
        }
        const decision = closeDecision("close", keep, tray === true);
        if (decision === "closes") return;
        ending.current = "close";
        if (decision === "asks-keep") {
          e.preventDefault();
          setAsking(true);
          return;
        }
        const asked = await firstQuestion();
        if (quitRequests.current !== requestsSeen) {
          e.preventDefault();
          return;
        }
        if (asked === null) return;
        e.preventDefault();
        setPrompt(asked);
      })
      .then((u) => (disposed ? u() : (unlisten = u)));

    // The flag is taken only once the listener stands: a request that lands in
    // between would otherwise be set, announced to nobody, and never taken.
    (deps.listen?.(QUIT_REQUEST_EVENT, () => void takeQuitRequest()) ?? Promise.resolve(undefined))
      .then((u) => (disposed ? u?.() : (unlistenQuit = u)))
      .catch((err: unknown) => {
        console.error("listening for quit requests failed:", err);
      })
      .then(() => {
        if (!disposed) void takeQuitRequest();
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
    keepRefused,
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
    cancel: () => {
      quitPromptShown.current = false;
      setPrompt(null);
    },
    // The answer is the switch. A write that fails leaves the window and the
    // question open, saying why: closing into a state nothing recorded would ask
    // again blind, and a question that vanishes reads as a cancel.
    keepCapturing: async () => {
      const requestsSeen = quitRequests.current;
      setKeepRefused(null);
      try {
        await deps.invoke("keep_capturing_write", { on: true });
      } catch (err) {
        console.error("storing keep capturing failed:", err);
        if (quitRequests.current === requestsSeen) setKeepRefused(refusalWords(err));
        return;
      }
      setAsking(false);
      if (quitRequests.current === requestsSeen) await destroy();
    },
    // Quit is the user's answer even where it could not be stored: the next
    // close simply asks again.
    quitInstead: async () => {
      const requestsSeen = quitRequests.current;
      setAsking(false);
      setKeepRefused(null);
      ending.current = "close";
      await deps.invoke("keep_capturing_write", { on: false }).catch((err: unknown) => {
        console.error("storing keep capturing failed:", err);
      });
      if (quitRequests.current !== requestsSeen) return;
      const asked = await question.current();
      if (quitRequests.current !== requestsSeen) return;
      if (asked === null) await destroy();
      else setPrompt(asked);
    },
    cancelAsk: () => {
      setAsking(false);
      setKeepRefused(null);
    },
  };
}
