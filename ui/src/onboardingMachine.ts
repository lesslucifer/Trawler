// Onboarding state machine — pure, no React, no I/O, so its transitions are
// unit-testable with no network (E-UISETUP.W1). The surface renders from it;
// W4 swaps the mount placeholder for a real scheduled capture self-test.

import type { PlatformId } from "./setup";

export type Step = "trust-ca" | "set-proxy" | "verify";
export type Phase = "checking" | "onboarding" | "proven";
export type DotStatus = "broken" | "checking" | "connected";

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
  | { t: "re-invoke"; at: number; another?: boolean } // tap status dot / "set up another device" → re-setup
  | { t: "choose"; platform: PlatformId }
  | { t: "exit" }; // Back to flows — returns to proven iff everProven, else no-op

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
      return { ...s, phase: "proven", everProven: true };
    case "not-detected":
      return { ...s, phase: "onboarding", step: "trust-ca", since: e.at };
    case "re-invoke":
      return { ...s, phase: "onboarding", step: "trust-ca", since: e.at, another: e.another ?? false };
    case "choose":
      return { ...s, chosen: e.platform };
    case "exit":
      return s.everProven ? { ...s, phase: "proven" } : s;
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

// Re-entering setup starts a new session; it does not stop capture that has
// already been proven from working (R5).
export function dotStatus(s: OnbState): DotStatus {
  if (s.phase === "proven" || s.everProven) return "connected";
  return s.phase === "checking" ? "checking" : "broken";
}

export const COPY = {
  back: "back to flows",
  dotTitle: {
    broken: "Not capturing yet — click to set up",
    checking: "Checking…",
    connected: "Capturing — click to run setup again",
  },
} as const;
