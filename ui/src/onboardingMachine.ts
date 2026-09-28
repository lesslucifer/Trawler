// Onboarding state machine — pure, no React, no I/O, so its transitions are
// unit-testable with no network (E-UISETUP.W1). The surface renders from it;
// W4 swaps the mount placeholder for a real scheduled capture self-test.

import type { PlatformId } from "./setup";

export type Step = "trust-ca" | "set-proxy" | "verify";
export type Phase = "checking" | "onboarding" | "list";
export type DotStatus = "broken" | "checking" | "quiet" | "connected";
export type DotOpens = "setup" | "list";

// `since` is when this setup session opened (epoch ms), null before one has:
// a request that started earlier is not news about the session. `chosen` is
// the platform last picked in this app session, and `another` whether this
// session was opened to set up another device.
export interface OnbState {
  phase: Phase;
  step: Step;
  everProven: boolean;
  since: number | null;
  chosen: PlatformId;
  another: boolean;
}

export type OnbEvent =
  | { t: "goto"; step: Step }
  | { t: "captured" } // fresh decrypted HTTPS flow — W4 detector dispatches
  | { t: "not-detected"; at: number } // launch self-test answered with nothing that proves capture
  | { t: "re-invoke"; at: number; another?: boolean } // the dot or "set up capture" from the list / "set up another device" → re-setup
  | { t: "choose"; platform: PlatformId }
  | { t: "exit" }; // "show traffic", or the dot from setup or the launch check — the list, proven or not

export const STEPS: readonly Step[] = ["trust-ca", "set-proxy", "verify"];

export function initial(): OnbState {
  return {
    phase: "checking",
    step: "trust-ca",
    everProven: false,
    since: null,
    chosen: "this",
    another: false,
  };
}

export function reduce(s: OnbState, e: OnbEvent): OnbState {
  switch (e.t) {
    case "goto":
      return { ...s, step: e.step };
    case "captured":
      return { ...s, phase: "list", everProven: true };
    case "not-detected":
      return { ...s, phase: "onboarding", step: "trust-ca", since: e.at };
    case "re-invoke":
      return { ...s, phase: "onboarding", step: "trust-ca", since: e.at, another: e.another ?? false };
    case "choose":
      return { ...s, chosen: e.platform };
    case "exit":
      return { ...s, phase: "list" };
  }
}

// "done" is proven by capture order, never self-reported: steps before the
// current one read done, the current one active, the rest pending.
export type StepStatus = "done" | "active" | "pending";
export function stepStatus(s: OnbState, step: Step): StepStatus {
  const cur = STEPS.indexOf(s.step);
  const idx = STEPS.indexOf(step);
  return idx < cur ? "done" : idx === cur ? "active" : "pending";
}

// The platform the setup screen answers when it opens, or null for the question
// left open. Asked to set up another device, it may not answer the one this
// machine already is (R6).
export function opensOn(s: OnbState): PlatformId | null {
  return s.another && s.chosen === "this" ? null : s.chosen;
}

// The dot switches screens in every phase (DEC-61): the list opens setup, and
// setup or the launch check opens the list.
export function dotOpens(s: OnbState): DotOpens {
  return s.phase === "list" ? "setup" : "list";
}

export function dotEvent(s: OnbState, at: number): OnbEvent {
  return dotOpens(s) === "setup" ? { t: "re-invoke", at } : { t: "exit" };
}

export const QUIET_AFTER_MS = 180_000;

// A capture proves the proxy worked then, not that it works now (DEC-61): a
// setting changed outside Trawler leaves nothing behind but silence. So the dot
// reads the newest captured request, http or https, and not the phase.
// `lastCaptureAt` is undefined until the core has answered, null when it has
// captured nothing.
export function dotStatus(lastCaptureAt: number | null | undefined, now: number): DotStatus {
  if (lastCaptureAt === undefined) return "checking";
  if (lastCaptureAt === null) return "broken";
  return now - lastCaptureAt < QUIET_AFTER_MS ? "connected" : "quiet";
}

export const COPY = {
  back: "show traffic",
  dotTitle: {
    broken: "Nothing captured yet",
    checking: "Checking…",
    quiet: "Nothing captured in the last 3 minutes",
    connected: "Capturing",
  },
  dotAction: {
    setup: "click to set up",
    list: "click to show traffic",
  },
} as const;
